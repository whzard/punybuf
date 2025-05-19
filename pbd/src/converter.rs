/*
	schema:
	{
		includes_common: boolean
		types: {
			name: string
			layer: number
			is: "struct" | "enum" | "alias"
			generic_args: string[]
			attrs: Attrs
			doc: string
			inline_owner?: String

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
			ret?: Ref
			err: {
				name: string
				discriminant: number
				attrs: Attrs
				doc: string
				value?: Ref
			}[]
		}[]
	}

	Ref = [name: string, layer: number | null, generic_args: Ref[]]
	Attrs = Record<string, string | null>
*/

use std::collections::HashMap;

use crate::flattener::{PBCommandArg, PBCommandDef, PBEnumVariant, PBField, PBTypeDef, PBTypeRef, PunybufDefinition};

fn convert_attrs(attrs: &HashMap<String, Option<String>>) -> json::JsonValue {
	let mut obj = json::JsonValue::new_object();
	for (k, v) in attrs {
		obj.insert(&k, match v { None => json::Null, Some(s) => s.as_str().into() }).unwrap();
	};

	obj
}

fn convert_ref(refr: &PBTypeRef) -> json::JsonValue {
	json::array![
		refr.reference.as_str(),
		refr.resolved_layer,
		refr.generics.iter().map(|r| convert_ref(r)).collect::<Vec<_>>()
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
		name: tp.get_name().0.as_str(),
		layer: *tp.get_layer(),
		generic_args: tp.get_generics().0.as_slice(),
		attrs: convert_attrs(tp.get_attrs()),
		doc: tp.get_doc().as_str(),
		inline_owner: tp.get_inline_owner().as_ref().map(|x| x.0.as_str()),
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
		err: convert_enum_variants(&cmd.err)
	}
}

pub fn convert_full_definition(def: &PunybufDefinition) -> String {
	json::stringify(json::object! {
		includes_common: def.includes_common,
		types: def.types.iter().map(convert_type).collect::<Vec<_>>(),
		commands: def.commands.iter().map(convert_command).collect::<Vec<_>>(),
	})
}