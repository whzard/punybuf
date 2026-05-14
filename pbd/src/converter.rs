/*
	schema:
	{
		includes_common: boolean
		types: {
			name: string
			layer: number
			is: "struct" | "enum" | "alias"
			generic_params: string[]
			attrs: Attrs
			doc: string
			inline_owner?: String
			is_highest_layer: boolean

			// is alias?
			alias?: Ref

			// is struct?
			fields?: {
				name: string
				attrs: Attrs
				doc: string
				value: Ref
				flags?: {
					name: string
					attrs: Attrs
					doc: string
					value?: Ref
				}[]
			}[]

			// is enum?
			variants?: {
				name: string
				discriminant: number
				attrs: Attrs
				doc: string
				value?: Ref
			}[]
		}[]
		commands: {
			name: string
			layer: number
			id: number
			doc: string
			attrs: Attrs
			is_highest_layer: boolean

			argument?: {
				is: "ref" | "struct"

				// is ref?
				ref?: Ref

				// is struct?
				fields?: {
					name: string
					attrs: Attrs
					doc: string
					value: Ref
					flags?: {
						name: string
						attrs: Attrs
						doc: string
						value?: Ref
					}[]
				}[]
			}
			ret: Ref
			err: {
				name: string
				discriminant: number
				attrs: Attrs
				doc: string
				value?: Ref
			}[]
		}[]
	}

	Ref = [name: string, layer: number | null, generic_params: Ref[], is_highest_layer: boolean]
	Attrs = Record<string, string | null>
*/

use std::collections::HashMap;

use json::JsonValue;

use crate::{flattener::{
	PBCommandArg, PBCommandDef, PBEnumVariant, PBField,
	PBFieldFlag, PBTypeDef, PBTypeRef, PunybufDefinition
}, lexer::Span};

fn convert_attrs(attrs: &HashMap<String, Option<String>>) -> json::JsonValue {
	let mut obj = json::JsonValue::new_object();
	// We sort this just so that the test suite can deterministically
	// compare json values as strings, without parsing them.
	// Implementations should not rely on this behavior!
	//
	// TODO: fix this so that the actual pbd command doesn't waste resources.
	let mut pairs = attrs.iter().collect::<Vec<_>>();
	pairs.sort();
	for (k, v) in pairs {
		obj.insert(&k, match v { None => json::Null, Some(s) => s.as_str().into() }).unwrap();
	};

	obj
}

fn convert_ref(refr: &PBTypeRef) -> json::JsonValue {
	json::array![
		refr.reference.as_str(),
		refr.resolved_layer,
		refr.generics.iter().map(|r| convert_ref(r)).collect::<Vec<_>>(),
		refr.is_highest_layer
	]
}

fn convert_fields(fields: &Vec<PBField>) -> json::JsonValue {
	json::JsonValue::from(
		fields.iter()
			.map(|v| {
				json::object! {
					name: v.name.as_str(),
					attrs: convert_attrs(&v.attrs),
					doc: v.doc.as_str(),
					value: convert_ref(&v.value),
					flags: v.flags.as_ref().map(|flags| {
						json::JsonValue::from(
							flags.iter()
							.map(|flag| {
								json::object! {
									name: flag.name.as_str(),
									attrs: convert_attrs(&flag.attrs),
									doc: flag.doc.as_str(),
									value: flag.value.as_ref().map(convert_ref)
								}
							})
							.collect::<Vec<_>>()
						)
					})
				}
			})
			.collect::<Vec<_>>()
	)
}

fn convert_enum_variants(variants: &Vec<PBEnumVariant>) -> json::JsonValue {
	json::JsonValue::from(
		variants.iter()
			.map(|v| {
				json::object! {
					name: v.name.as_str(),
					discriminant: v.discriminant,
					attrs: convert_attrs(&v.attrs),
					doc: v.doc.as_str(),
					value: v.value.as_ref().map(|rf| convert_ref(rf))
				}
			})
			.collect::<Vec<_>>()
	)
}

fn convert_type(tp: &PBTypeDef) -> json::JsonValue {
	let mut obj = json::object! {
		name: tp.get_name().0,
		layer: *tp.get_layer(),
		generic_params: tp.get_generics().0.as_slice(),
		attrs: convert_attrs(tp.get_attrs()),
		doc: tp.get_doc(),
		inline_owner: tp.get_inline_owner().as_ref().map(|x| x.0.as_str()),
		is_highest_layer: tp.is_highest_layer(),
	};

	match tp {
		PBTypeDef::Alias { alias, .. } => {
			obj.insert("is", "alias").unwrap();
			obj.insert("alias", convert_ref(alias)).unwrap();
		}
		PBTypeDef::Struct { fields, .. } => {
			obj.insert("is", "struct").unwrap();
			obj.insert("fields", convert_fields(fields)).unwrap();
		}
		PBTypeDef::Enum { variants, .. } => {
			obj.insert("is", "enum").unwrap();
			obj.insert("variants", convert_enum_variants(variants)).unwrap();
		}
	}

	obj
}

fn convert_command(cmd: &PBCommandDef) -> json::JsonValue {
	let mut arg = json::object! {};

	match &cmd.argument {
		PBCommandArg::Ref(refr) => {
			arg.insert("is", "ref").unwrap();
			arg.insert("ref", convert_ref(refr)).unwrap();
		}
		PBCommandArg::Struct { fields } => {
			arg.insert("is", "struct").unwrap();
			arg.insert("fields", convert_fields(fields)).unwrap();
		}
		PBCommandArg::None => {}
	}

	json::object! {
		name: cmd.name.as_str(),
		layer: cmd.layer,
		id: cmd.command_id,
		attrs: convert_attrs(&cmd.attrs),
		doc: cmd.doc.as_str(),
		arg: arg,
		ret: convert_ref(&cmd.ret),
		err: convert_enum_variants(&cmd.err),
		is_highest_layer: cmd.is_highest_layer
	}
}

pub fn convert_full_definition(def: &PunybufDefinition) -> String {
	json::stringify(json::object! {
		includes_common: def.includes_common,
		types: def.types.iter().map(convert_type).collect::<Vec<_>>(),
		commands: def.commands.iter().map(convert_command).collect::<Vec<_>>(),
	})
}

pub fn from_json(input: &str) -> Result<PunybufDefinition, String> {
	let mut object = json::parse(input).map_err(|e| e.to_string())?;
	let includes_common = object.remove("includes_common").as_bool().unwrap_or(false);
	let mut object_types = object.remove("types");
	let mut result = PunybufDefinition::new(includes_common);
	for obj_typ in object_types.members_mut() {
		result.types.push(type_from_json(obj_typ)?);
	}
	let mut object_commands = object.remove("commands");
	for obj_cmd in object_commands.members_mut() {
		result.commands.push(cmd_from_json(obj_cmd)?);
	}
	Ok(result)
}

fn type_from_json(obj_typ: &mut JsonValue) -> Result<PBTypeDef, String> {
	match obj_typ.remove("is").as_str().unwrap_or("<nothing>") {
		"struct" => {
			Ok(PBTypeDef::Struct {
				name: obj_typ.remove("name").to_string(),
				name_span: Span::impossible(),
				doc: obj_typ.remove("doc").to_string(),
				layer: obj_typ.remove("layer").as_u32().unwrap_or(0),
				attrs: attrs_from_json(&mut obj_typ.remove("attrs")),
				generic_params: obj_typ.remove("generic_params").members().map(|v| {
					v.to_string()
				}).collect(),
				generic_span: Span::impossible(),
				fields: fields_from_json(&mut obj_typ.remove("fields"))?,
				inline_owner: obj_typ.remove("inline_owner").as_str()
					.map(|x| (x.to_string(), Span::impossible())),
				is_highest_layer: obj_typ.remove("is_highest_owner").as_bool().unwrap_or(false)
			})
		}
		"enum" => {
			Ok(PBTypeDef::Enum {
				name: obj_typ.remove("name").to_string(),
				name_span: Span::impossible(),
				doc: obj_typ.remove("doc").to_string(),
				layer: obj_typ.remove("layer").as_u32().unwrap_or(0),
				attrs: attrs_from_json(&mut obj_typ.remove("attrs")),
				generic_params: obj_typ.remove("generic_params").members().map(|v| {
					v.to_string()
				}).collect(),
				generic_span: Span::impossible(),
				variants: variants_from_json(&mut obj_typ.remove("variants"))?,
				inline_owner: obj_typ.remove("inline_owner").as_str()
					.map(|x| (x.to_string(), Span::impossible())),
				is_highest_layer: obj_typ.remove("is_highest_owner").as_bool().unwrap_or(false)
			})
		}
		"alias" => {
			Ok(PBTypeDef::Alias {
				name: obj_typ.remove("name").to_string(),
				name_span: Span::impossible(),
				doc: obj_typ.remove("doc").to_string(),
				layer: obj_typ.remove("layer").as_u32().unwrap_or(0),
				attrs: attrs_from_json(&mut obj_typ.remove("attrs")),
				generic_params: obj_typ.remove("generic_params").members().map(|v| {
					v.to_string()
				}).collect(),
				generic_span: Span::impossible(),
				alias: ref_from_json(&mut obj_typ.remove("alias"))?,
				is_highest_layer: obj_typ.remove("is_highest_owner").as_bool().unwrap_or(false)
			})
		}
		_ => {
			Err("invalid `is` value".into())
		}
	}
}

fn cmd_from_json(obj_cmd: &mut JsonValue) -> Result<PBCommandDef, String> {
	Ok(PBCommandDef {
		name: obj_cmd.remove("name").to_string(),
		name_span: Span::impossible(),
		argument: arg_from_json(&mut obj_cmd.remove("argument"))?,
		argument_span: Span::impossible(),
		attrs: attrs_from_json(&mut obj_cmd.remove("attrs")),
		doc: obj_cmd.remove("doc").to_string(),
		layer: obj_cmd.remove("layer").as_u32().unwrap_or(0),
		command_id: obj_cmd.remove("id").as_u32().ok_or("invalid command id")?,
		ret: ref_from_json(&mut obj_cmd.remove("ret"))?,
		err: variants_from_json(&mut obj_cmd.remove("err"))?,
		err_span: Span::impossible(),
		is_highest_layer: obj_cmd.remove("is_highest_layer").as_bool().unwrap_or(false)
	})
}

fn arg_from_json(obj_arg: &mut JsonValue) -> Result<PBCommandArg, String> {
	if obj_arg.is_null() {
		return Ok(PBCommandArg::None);
	}
	match obj_arg.remove("is").as_str().unwrap_or("<unknown>") {
		"ref" => {
			Ok(PBCommandArg::Ref(ref_from_json(&mut obj_arg.remove("ref"))?))
		}
		"struct" => {
			Ok(PBCommandArg::Struct {
				fields: fields_from_json(&mut obj_arg.remove("fields"))?
			})
		}
		_ => {
			Err("invalid `is` value in a command".into())
		}
	}
}

fn attrs_from_json(obj_attrs: &mut JsonValue) -> HashMap<String, Option<String>> {
	let mut result = HashMap::new();
	for (name, val) in obj_attrs.entries() {
		result.insert(
			name.into(),
			if let Some(v) = val.as_str() { Some(v.into()) } else { None }
		);
	}
	result
}

fn fields_from_json(obj_fields: &mut JsonValue) -> Result<Vec<PBField>, String> {
	let mut fields = vec![];
	for obj_field in obj_fields.members_mut() {
		fields.push(PBField {
			name: obj_field.remove("name").to_string(),
			name_span: Span::impossible(),
			value: ref_from_json(&mut obj_field.remove("value"))?,
			flags: flags_from_json(&mut obj_field.remove("flags"))?,
			attrs: attrs_from_json(&mut obj_field.remove("attrs")),
			doc: obj_field.remove("doc").to_string()
		});
	}
	Ok(fields)
}

fn flags_from_json(obj_flags: &mut JsonValue) -> Result<Option<Vec<PBFieldFlag>>, String> {
	if obj_flags.is_null() {
		return Ok(None);
	}
	let mut flags = vec![];
	for obj_flag in obj_flags.members_mut() {
		flags.push(PBFieldFlag {
			name: obj_flag.remove("name").to_string(),
			name_span: Span::impossible(),
			value: if let mut val = obj_flag.remove("value") && !val.is_null() {
				Some(ref_from_json(&mut val)?)
			} else {
				None
			},
			attrs: attrs_from_json(&mut obj_flag.remove("attrs")),
			doc: obj_flag.remove("doc").to_string()
		});
	}
	Ok(Some(flags))
}

fn variants_from_json(obj_variants: &mut JsonValue) -> Result<Vec<PBEnumVariant>, String> {
	let mut variants = vec![];
	for obj_var in obj_variants.members_mut() {
		variants.push(PBEnumVariant {
			name: obj_var.remove("name").to_string(),
			name_span: Span::impossible(),
			discriminant: obj_var.remove("discriminant").as_u8().ok_or("invalid discriminant")?,
			value: if let mut val = obj_var.remove("value") && !val.is_null() {
				Some(ref_from_json(&mut val)?)
			} else {
				None
			},
			attrs: attrs_from_json(&mut obj_var.remove("attrs")),
			doc: obj_var.remove("doc").to_string()
		});
	}
	Ok(variants)
}

fn ref_from_json(obj_ref: &mut JsonValue) -> Result<PBTypeRef, String> {
	// Ref = [name: string, layer: number | null, generic_params: Ref[], is_highest_layer: boolean]
	let mut iter = obj_ref.members_mut();
	let name = iter.next().ok_or("invalid reference: no name")?.to_string();
	let layer = iter.next().ok_or("invalid reference: no layer")?.as_u32()
		.ok_or("invalid reference: incorrect layer")?;
	let obj_generic_params = iter.next().ok_or("invalid reference: no generic_params")?;
	let mut generic_params = vec![];
	for obj_ref in obj_generic_params.members_mut() {
		generic_params.push(ref_from_json(obj_ref)?);
	}
	let is_highest_layer = iter.next().ok_or("invalid reference: no is_highest_layer")?
		.as_bool().unwrap_or(false);
	Ok(PBTypeRef {
		reference: name,
		reference_span: Span::impossible(),
		generics: generic_params,
		generic_span: Span::impossible(),
		resolved_layer: Some(layer),
		is_highest_layer,
		// TODO: currently not included in json
		is_global: true,
	})
}