use std::collections::HashMap;
use crc::{Crc, CRC_32_CKSUM};

use crate::{errors::{parser_err, ExtendedErrorExplanation, PunybufError}, lexer::Span, parser::{CommandArgument, Declaration, DeclarationValue, EnumVariant, Field, FlexibleDeclarationValue, ValueEnumVariant, ValueReference}};

pub const PB_CRC: Crc<u32> = Crc::<u32>::new(&CRC_32_CKSUM);

#[derive(Debug, Clone)]
pub struct PBTypeRef {
	pub reference: String,
	pub reference_span: Span,
	pub generics: Vec<PBTypeRef>,
	pub generic_span: Span,
	/// the actual layer of the type this is referring to
	/// 
	/// (this is only valid post-resolution)
	pub resolved_layer: Option<u32>,
	/// is this the highest layer?
	/// 
	/// (this is only valid post-resolution)
	pub is_highest_layer: bool,
	/// is it global or generic?
	/// 
	/// (this is only valid post-resolution)
	pub is_global: bool,
}

#[derive(Debug, Clone)]
pub struct PBFieldFlag {
	pub name: String,
	pub name_span: Span,
	pub value: Option<PBTypeRef>,
	pub attrs: HashMap<String, Option<String>>,
	pub doc: String,
}

#[derive(Debug, Clone)]
pub struct PBField {
	pub name: String,
	pub name_span: Span,
	pub value: PBTypeRef,
	pub flags: Option<Vec<PBFieldFlag>>,
	pub attrs: HashMap<String, Option<String>>,
	pub doc: String,
}

#[derive(Debug, Clone)]
pub struct PBEnumVariant {
	pub name: String,
	pub name_span: Span,
	pub discriminant: u8,
	pub value: Option<PBTypeRef>,
	pub attrs: HashMap<String, Option<String>>,
	pub doc: String,
}

#[derive(Debug, Clone)]
pub enum PBTypeDef {
	Struct {
		name: String,
		name_span: Span,
		doc: String,
		layer: u32,
		attrs: HashMap<String, Option<String>>,
		generic_params: Vec<String>,
		generic_span: Span,
		fields: Vec<PBField>,
		inline_owner: Option<(String, Span)>,
		is_highest_layer: bool,
	},
	Enum {
		name: String,
		name_span: Span,
		doc: String,
		layer: u32,
		attrs: HashMap<String, Option<String>>,
		generic_params: Vec<String>,
		generic_span: Span,
		variants: Vec<PBEnumVariant>,
		inline_owner: Option<(String, Span)>,
		is_highest_layer: bool,
	},
	Alias {
		name: String,
		name_span: Span,
		doc: String,
		layer: u32,
		attrs: HashMap<String, Option<String>>,
		generic_params: Vec<String>,
		generic_span: Span,
		alias: PBTypeRef,
		is_highest_layer: bool,
	}
}

impl PBTypeDef {
	pub fn get_name(&self) -> (&str, &Span) {
		match self {
			Self::Alias { name, name_span, .. } |
			Self::Enum { name, name_span, .. } |
			Self::Struct { name, name_span, .. } => (name, name_span)
		}
	}
	pub fn get_generics(&self) -> (&Vec<String>, &Span) {
		match self {
			Self::Alias { generic_params, generic_span, .. } |
			Self::Enum { generic_params, generic_span, .. } |
			Self::Struct { generic_params, generic_span, .. } => (generic_params, generic_span)
		}
	}
	pub fn get_inline_owner(&self) -> &Option<(String, Span)> {
		match self {
			Self::Alias { .. } => &None,
			Self::Enum { inline_owner, .. } |
			Self::Struct { inline_owner, .. } => inline_owner
		}
	}
	pub fn get_attrs(&self) -> &HashMap<String, Option<String>> {
		match self {
			Self::Alias { attrs, .. } |
			Self::Enum { attrs, .. } |
			Self::Struct { attrs, .. } => attrs
		}
	}
	pub fn get_doc(&self) -> &str {
		match self {
			Self::Alias { doc, .. } |
			Self::Enum { doc, .. } |
			Self::Struct { doc, .. } => doc
		}
	}
	pub fn get_layer(&self) -> &u32 {
		match self {
			Self::Alias { layer, .. } |
			Self::Enum { layer, .. } |
			Self::Struct { layer, .. } => layer
		}
	}
	pub fn is_highest_layer(&self) -> bool {
		match self {
			Self::Alias { is_highest_layer, .. } |
			Self::Enum { is_highest_layer, .. } |
			Self::Struct { is_highest_layer, .. } => *is_highest_layer
		}
	}
}

#[derive(Debug, Clone)]
pub enum PBCommandArg {
	None,
	Ref(PBTypeRef),
	Struct {
		fields: Vec<PBField>,
	}
}

#[derive(Debug, Clone)]
pub struct PBCommandDef {
	pub name: String,
	pub name_span: Span,
	pub argument: PBCommandArg,
	pub argument_span: Span,
	pub attrs: HashMap<String, Option<String>>,
	pub doc: String,
	pub layer: u32,
	pub command_id: u32,
	pub ret: PBTypeRef,
	pub err: Vec<PBEnumVariant>,
	pub err_span: Span,
	pub is_highest_layer: bool,
}

#[derive(Debug, Clone)]
pub struct PunybufDefinition {
	pub types: Vec<PBTypeDef>,
	pub commands: Vec<PBCommandDef>,
	pub includes_common: bool,
	context_inline_owner: Option<(String, Span)>,
}

impl PunybufDefinition {
	fn new(includes_common: bool) -> Self {
		Self {
			types: vec![],
			commands: vec![],
			includes_common,
			context_inline_owner: None,
		}
	}
}

impl PunybufDefinition {
	pub fn flatten_doc(&self, doc: String) -> String {
		let mut result = String::with_capacity(doc.len());
		let mut is_empty_first_line = false;
		let mut is_skipping_empty_lines = true;
		let mut remove_whitespace: Option<usize> = None;

		for line in doc.lines() {
			if is_skipping_empty_lines && line.chars().all(|c| c.is_whitespace()) {
				is_empty_first_line = true;
				is_skipping_empty_lines = true;
				continue;
			}
			is_skipping_empty_lines = false;
			if is_empty_first_line {
				if let None = remove_whitespace {
					let mut whitespace_count: usize = 0;
					for c in line.chars() {
						if c.is_whitespace() {
							whitespace_count += 1;
						} else { break }
					}
					remove_whitespace = Some(whitespace_count)
				}

				let Some(mut whitespace_count) = &remove_whitespace else { continue };

				for (char_index, c) in line.chars().enumerate() {
					if !c.is_whitespace() {
						// short-circuit if not a whitespace
						whitespace_count = 0;
					}
					if char_index >= whitespace_count {
						result.push(c);
					}
				}

			} else {
				result.push_str(line.trim());
			}
			result.push('\n');
		}
		//result.pop(); // trailing newline
		while result.chars().next_back() == Some('\n') {
			result.pop();
		}
		result
	}
	pub fn flatten_reference(&mut self, refr: ValueReference) -> PBTypeRef {
		match refr {
			ValueReference::Reference { name, name_span, generics, generic_span, .. } => {
				let generics = generics.into_iter().map(|r| self.flatten_reference(r)).collect();
				PBTypeRef {
					reference: name.to_string(),
					reference_span: name_span,
					generics, generic_span, resolved_layer: None,
					is_global: true,
					is_highest_layer: false
				}
			}
			ValueReference::InlineDeclaration { symbol, name_span, decl, .. } => {
				self.flatten_flexible_decl(
					symbol.to_string(), name_span.clone(),
					"".to_string(),
					// TODO: add an ability to add attributes to
					// inline declarations
					HashMap::new(),
					decl, vec![],
					Span::impossible()
				);
				PBTypeRef {
					reference: symbol,
					reference_span: name_span,
					generics: vec![],
					generic_span: Span::impossible(),
					resolved_layer: None,
					is_global: true,
					is_highest_layer: false
				}
			}
		}
	}
	pub fn flatten_field(&mut self, field: Field) -> PBField {
		let flags = field.flags.map(|flags| flags.into_iter().map(|f| {
			PBFieldFlag {
				name: f.name, name_span: f.name_span,
				value: f.value.map(|rf| self.flatten_reference(rf)),
				attrs: f.attrs, doc: self.flatten_doc(f.doc)
			}
		}).collect());

		PBField {
			name: field.name, name_span: field.name_span,
			value: self.flatten_reference(field.value),
			flags, attrs: field.attrs, doc: self.flatten_doc(field.doc)
		}
	}
	pub fn flatten_enum_variant(&mut self, ev: EnumVariant) -> PBEnumVariant {
		PBEnumVariant {
			name: ev.name, name_span: ev.name_span,
			discriminant: ev.discriminant,
			value: ev.value.map(|rf| self.flatten_reference(rf)),
			attrs: ev.attrs, doc: self.flatten_doc(ev.doc)
		}
	}
	pub fn flatten_value_enum_variant(&mut self, vev: ValueEnumVariant) -> PBEnumVariant {
		let name = vev.value.get_name().to_string();
		let name_span = vev.value.get_name_span().clone();
		PBEnumVariant {
			name, name_span,
			discriminant: vev.discriminant,
			value: Some(self.flatten_reference(vev.value)),
			attrs: vev.attrs, doc: self.flatten_doc(vev.doc)
		}
	}
	pub fn flatten_flexible_decl(
		&mut self,
		name: String, name_span: Span,
		doc: String, attrs: HashMap<String, Option<String>>,
		decl: FlexibleDeclarationValue,
		generic_params: Vec<String>, generic_span: Span
	) {
		// Rust annoyance: the next line fails without `.clone()` but
		// shouldn't because we just reassign it on the next line.
		// I think that's because Rust doesn't differentiate between moving `self`
		// and moving `self.something`.
		let inline_owner = self.context_inline_owner.clone();
		let revert_owner = inline_owner.clone();
		self.context_inline_owner = Some((name.clone(), name_span.clone()));
		match decl {
			FlexibleDeclarationValue::EnumDeclaration { inline, layer, variants } => {
				if inline_owner == None && inline {
					panic!("bad state: root-level declaration marked inline")
				}
				let variants = variants.into_iter().map(|ev| self.flatten_enum_variant(ev)).collect();
				self.types.push(PBTypeDef::Enum {
					name, name_span,
					doc: self.flatten_doc(doc), attrs,
					generic_params, generic_span,
					variants, layer,
					inline_owner,
					is_highest_layer: false,
				})
			}
			FlexibleDeclarationValue::StructDeclaration { inline, layer, fields } => {
				if inline_owner == None && inline {
					panic!("bad state: root-level declaration marked inline")
				}
				let fields = fields.into_iter().map(|f| self.flatten_field(f)).collect();
				self.types.push(PBTypeDef::Struct {
					name, name_span,
					doc: self.flatten_doc(doc), attrs,
					generic_params, generic_span,
					fields, layer,
					inline_owner,
					is_highest_layer: false,
				})
			}
			FlexibleDeclarationValue::ValueEnumDeclaration { inline, layer, variants } => {
				if inline_owner == None && inline {
					panic!("bad state: root-level declaration marked inline")
				}
				let variants = variants.into_iter().map(|ev| self.flatten_value_enum_variant(ev)).collect();
				self.types.push(PBTypeDef::Enum {
					name, name_span,
					doc: self.flatten_doc(doc), attrs,
					generic_params, generic_span,
					variants, layer,
					inline_owner,
					is_highest_layer: false,
				})
			}
		}
		self.context_inline_owner = revert_owner;
	}
}

pub fn flatten(decls: Vec<Declaration>, includes_common: bool) -> Result<PunybufDefinition, PunybufError> {
	let mut def = PunybufDefinition::new(includes_common);

	for decl in decls {
		match decl.value {
			DeclarationValue::CommandDeclaration { argument, argument_span, layer, ret, err, err_span } => {
				let pb_arg = match argument {
					CommandArgument::None => PBCommandArg::None,
					CommandArgument::Reference(refr) => PBCommandArg::Ref(def.flatten_reference(refr)),
					CommandArgument::Struct { fields } => {
						PBCommandArg::Struct {
							fields: fields.into_iter().map(|f| def.flatten_field(f)).collect()
						}
					}
				};

				let err = match err {
					Some(bx) => {
						match *bx {
							FlexibleDeclarationValue::StructDeclaration { .. } => {
								return Err(parser_err!(err_span, "errors are always enums (or value-enums), got a struct"));
							}
							FlexibleDeclarationValue::EnumDeclaration { variants, .. } => {
								variants.into_iter().map(|ev| def.flatten_enum_variant(ev)).collect()
							}
							FlexibleDeclarationValue::ValueEnumDeclaration { variants, .. } => {
								variants.into_iter().map(|ev| def.flatten_value_enum_variant(ev)).collect()
							}
						}
					},
					None => vec![]
				};

				let ret = def.flatten_reference(*ret);

				let command_id = PB_CRC.checksum(format!("{}.{}", decl.symbol, layer).as_bytes());

				def.commands.push(PBCommandDef {
					name: decl.symbol,
					name_span: decl.symbol_span,
					argument: pb_arg,
					attrs: decl.attrs,
					doc: def.flatten_doc(decl.doc),
					argument_span, layer,
					ret, err, err_span,
					command_id, is_highest_layer: false
				});
			}
			DeclarationValue::AliasDeclaration { generic_params, generic_span, alias, layer } => {
				let alias = def.flatten_reference(*alias);
				def.types.push(PBTypeDef::Alias {
					name: decl.symbol,
					name_span: decl.symbol_span,
					doc: def.flatten_doc(decl.doc),
					attrs: decl.attrs,
					layer, generic_params,
					generic_span, alias,
					is_highest_layer: false,
				});
			}
			DeclarationValue::Flexible { val, generic_params, generic_span, .. } => {
				def.flatten_flexible_decl(
					decl.symbol,
					decl.symbol_span,
					decl.doc, decl.attrs,
					val,
					generic_params, generic_span,
				);
			}
		}
	}

	Ok(def)
}