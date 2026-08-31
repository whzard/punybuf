use crc::{Crc, CRC_32_CKSUM};
use std::collections::HashMap;

use crate::{
	errors::{PunybufError, parser_err},
	lexer::Span,
	parser::{
		CommandArgument, Declaration, DeclarationValue, EnumVariant, Field,
		FlexibleDeclarationValue, ValueEnumVariant, ValueReference,
	}, pb_err,
};

pub(crate) const PB_CRC: Crc<u32> = Crc::<u32>::new(&CRC_32_CKSUM);

#[derive(Debug, Clone)]
#[allow(unused)]
pub struct PBTypeRef {
	pub(crate) reference: String,
	pub(crate) reference_span: Span,
	pub(crate) generics: Vec<PBTypeRef>,
	pub(crate) generic_span: Span,
	/// the actual layer of the type this is referring to
	/// 
	/// (this is only valid post-resolution)
	pub(crate) resolved_layer: Option<u32>,
	/// is this the highest layer?
	/// 
	/// (this is only valid post-resolution)
	pub(crate) is_highest_layer: bool,
	/// is it global or generic?
	/// 
	/// (this is only valid post-resolution)
	pub(crate) is_global: bool,
}

#[allow(dead_code)]
impl PBTypeRef {
	pub fn reference(&self) -> (&str, &Span) {
		(&self.reference, &self.reference_span)
	}
	/// Generic arguments of this reference, like `String` in `Array<String>`
	pub fn generics(&self) -> (&Vec<PBTypeRef>, &Span) {
		(&self.generics, &self.generic_span)
	}
	/// The resolved layer of the reference
	pub fn layer(&self) -> Option<u32> {
		// This is only available in the public API post-resolution
		self.resolved_layer
	}
	/// Whether the `.layer()` is the highest layer of the
	/// definition.
	pub fn is_highest_layer(&self) -> bool {
		// This is only available in the public API post-resolution
		self.is_highest_layer
	}
	/// Whether this references a generic parameter, like
	/// ```pbd
	/// Thing<T> = {
	///   inner: T  # <-- here
	/// }
	/// ```
	pub fn is_generic(&self) -> bool {
		// This is only available in the public API post-resolution
		!self.is_global
	}
}

#[derive(Debug, Clone)]
#[allow(unused)]
pub struct PBFieldFlag {
	pub(crate) name: String,
	pub(crate) name_span: Span,
	pub(crate) value: Option<PBTypeRef>,
	pub(crate) attrs: HashMap<String, Option<String>>,
	pub(crate) doc: String,
}

#[allow(dead_code)]
impl PBFieldFlag {
	pub fn name(&self) -> (&str, &Span) {
		(&self.name, &self.name_span)
	}
	pub fn value(&self) -> Option<&PBTypeRef> {
		self.value.as_ref()
	}
	pub fn attrs(&self) -> &HashMap<String, Option<String>> {
		&self.attrs
	}
	pub fn doc(&self) -> &str {
		&self.doc
	}
}

#[derive(Debug, Clone)]
#[allow(unused)]
pub struct PBField {
	pub(crate) name: String,
	pub(crate) name_span: Span,
	pub(crate) anonymous: bool,
	pub(crate) value: PBTypeRef,
	pub(crate) flags: Option<Vec<PBFieldFlag>>,
	pub(crate) attrs: HashMap<String, Option<String>>,
	pub(crate) doc: String,
}

#[allow(dead_code)]
impl PBField {
	pub fn name(&self) -> (&str, &Span) {
		(&self.name, &self.name_span)
	}
	pub fn value(&self) -> &PBTypeRef {
		&self.value
	}
	/// Whether this is a flag field and is "anonymous", as in defined like
	/// ```pbd
	///   UInt.{   # <-- this
	///     # ...
	///   }
	/// ```
	pub fn anonymous(&self) -> bool {
		self.anonymous
	}
	pub fn attrs(&self) -> &HashMap<String, Option<String>> {
		&self.attrs
	}
	pub fn doc(&self) -> &str {
		&self.doc
	}
	/// `Some()` if this is a flag field
	pub fn flags(&self) -> Option<&Vec<PBFieldFlag>> {
		self.flags.as_ref()
	}
}

#[derive(Debug, Clone)]
#[allow(unused)]
pub struct PBEnumVariant {
	pub(crate) name: String,
	pub(crate) name_span: Span,
	pub(crate) discriminant: u8,
	pub(crate) value: Option<PBTypeRef>,
	pub(crate) attrs: HashMap<String, Option<String>>,
	pub(crate) doc: String,
}

#[allow(dead_code)]
impl PBEnumVariant {
	pub fn name(&self) -> (&str, &Span) {
		(&self.name, &self.name_span)
	}
	pub fn discriminant(&self) -> u8 {
		self.discriminant
	}
	pub fn value(&self) -> Option<&PBTypeRef> {
		self.value.as_ref()
	}
	pub fn attrs(&self) -> &HashMap<String, Option<String>> {
		&self.attrs
	}
	pub fn doc(&self) -> &str {
		&self.doc
	}
}

// TODO: instead of including the same fields in all the
// variants of this enum, maybe create a new enum with just
// the "meat" of the type definition. Also, maybe expose that
// enum directly to the API
#[derive(Debug, Clone)]
#[allow(unused)]
pub(crate) enum PBTypeDef {
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
	pub(crate) fn get_name(&self) -> (&str, &Span) {
		match self {
			Self::Alias { name, name_span, .. } |
			Self::Enum { name, name_span, .. } |
			Self::Struct { name, name_span, .. } => (name, name_span)
		}
	}
	pub(crate) fn get_generics(&self) -> (&Vec<String>, &Span) {
		match self {
			Self::Alias { generic_params, generic_span, .. } |
			Self::Enum { generic_params, generic_span, .. } |
			Self::Struct { generic_params, generic_span, .. } => (generic_params, generic_span)
		}
	}
	pub(crate) fn get_inline_owner(&self) -> Option<&(String, Span)> {
		match self {
			Self::Alias { .. } => None,
			Self::Enum { inline_owner, .. } |
			Self::Struct { inline_owner, .. } => inline_owner.as_ref()
		}
	}
	pub(crate) fn get_attrs(&self) -> &HashMap<String, Option<String>> {
		match self {
			Self::Alias { attrs, .. } |
			Self::Enum { attrs, .. } |
			Self::Struct { attrs, .. } => attrs
		}
	}
	pub(crate) fn get_doc(&self) -> &str {
		match self {
			Self::Alias { doc, .. } |
			Self::Enum { doc, .. } |
			Self::Struct { doc, .. } => doc
		}
	}
	pub(crate) fn get_layer(&self) -> &u32 {
		match self {
			Self::Alias { layer, .. } |
			Self::Enum { layer, .. } |
			Self::Struct { layer, .. } => layer
		}
	}
	pub(crate) fn is_highest_layer(&self) -> bool {
		match self {
			Self::Alias { is_highest_layer, .. } |
			Self::Enum { is_highest_layer, .. } |
			Self::Struct { is_highest_layer, .. } => *is_highest_layer
		}
	}
}

/// A read-only view of a Punybuf type
pub struct PunybufType<'a>(&'a PBTypeDef);

#[allow(dead_code)]
impl<'s> PunybufType<'s> {
	pub fn name(&self) -> (&str, &Span) {
		self.0.get_name()
	}
	pub fn generics(&self) -> (&Vec<String>, &Span) {
		self.0.get_generics()
	}
	/// If defined inline, the type name and span of the type that
	/// defined this inline, for example:
	/// ```pbd
	/// Thing = {
	///   owned: Inlined {
	///     field: String
	///   }
	/// }
	/// ```
	/// ```ignore
	/// Inlined.inline_owner() // Some("Thing", ...)
	/// ```
	pub fn inline_owner(&self) -> Option<&(String, Span)> {
		self.0.get_inline_owner()
	}
	pub fn attrs(&self) -> &HashMap<String, Option<String>> {
		self.0.get_attrs()
	}
	pub fn doc(&self) -> &str {
		self.0.get_doc()
	}
	pub fn layer(&self) -> u32 {
		*self.0.get_layer()
	}
	/// Whether this type is defined at the highest layer
	pub fn is_highest_layer(&self) -> bool {
		self.0.is_highest_layer()
	}
	// TODO: the following API sucks. Not sure how to change it without
	// exposing all the enum values - Maybe see on PBTypeDef
	/// If this type is an enum, get its variants, `None` otherwise
	pub fn as_enum(&self) -> Option<&Vec<PBEnumVariant>> {
		match self.0 {
			PBTypeDef::Enum { variants, .. } => Some(&variants),
			_ => None
		}
	}
	/// If this type is a struct, get its fields, `None` otherwise
	pub fn as_struct(&self) -> Option<&Vec<PBField>> {
		match self.0 {
			PBTypeDef::Struct { fields, .. } => Some(&fields),
			_ => None
		}
	}
	/// If this type is an alias, get the aliased type, `None` otherwise
	pub fn as_alias(&self) -> Option<&PBTypeRef> {
		match self.0 {
			PBTypeDef::Alias { alias, .. } => Some(&alias),
			_ => None
		}
	}
}

#[derive(Debug, Clone)]
#[allow(unused)]
/// The argument to a Punybuf command.
pub enum PBCommandArg {
	/// No argument, like `cmd: () -> ...`
	None,
	/// A single reference, like `cmd: Type -> ...`
	Ref(PBTypeRef),
	/// A struct argument, like `cmd: { ... } -> ...`
	Struct {
		fields: Vec<PBField>,
	}
}

#[derive(Debug, Clone)]
#[allow(unused)]
pub(crate) struct PBCommandDef {
	pub(crate) name: String,
	pub(crate) name_span: Span,
	pub(crate) argument: PBCommandArg,
	pub(crate) argument_span: Span,
	pub(crate) attrs: HashMap<String, Option<String>>,
	pub(crate) doc: String,
	pub(crate) layer: u32,
	pub(crate) command_id: u32,
	pub(crate) ret: PBTypeRef,
	pub(crate) err: Vec<PBEnumVariant>,
	pub(crate) err_span: Span,
	pub(crate) is_highest_layer: bool,
}

/// A read-only view of a Punybuf command
pub struct PunybufCommand<'a>(&'a PBCommandDef);

#[allow(dead_code)]
impl<'s> PunybufCommand<'s> {
	pub fn name(&self) -> (&str, &Span) {
		(&self.0.name, &self.0.name_span)
	}
	pub fn argument(&self) -> (&PBCommandArg, &Span) {
		(&self.0.argument, &self.0.argument_span)
	}
	pub fn attrs(&self) -> &HashMap<String, Option<String>> {
		&self.0.attrs
	}
	pub fn doc(&self) -> &str {
		&self.0.doc
	}
	pub fn layer(&self) -> u32 {
		self.0.layer
	}
	pub fn id(&self) -> u32 {
		self.0.command_id
	}
	pub fn ret(&self) -> &PBTypeRef {
		&self.0.ret
	}
	pub fn err(&self) -> (&Vec<PBEnumVariant>, &Span) {
		(&self.0.err, &self.0.err_span)
	}
	pub fn is_highest_layer(&self) -> bool {
		self.0.is_highest_layer
	}
}

#[derive(Debug, Clone)]
#[allow(unused)]
pub struct PunybufDefinition {
	pub(crate) types: Vec<PBTypeDef>,
	pub(crate) commands: Vec<PBCommandDef>,
	pub(crate) includes_common: bool,
	context_inline_owner: Option<(String, Span)>,
}

impl PunybufDefinition {
	pub(crate) fn new(includes_common: bool) -> Self {
		Self {
			types: vec![],
			commands: vec![],
			includes_common,
			context_inline_owner: None,
		}
	}
}

// allow(dead_code_pub_in_binary) does not work for some reason
#[allow(dead_code)]
impl PunybufDefinition {
	/// Whether the definition has `include common` somewhere
	pub fn includes_common(&self) -> bool {
		self.includes_common
	}
	pub fn types(&self) -> impl Iterator<Item = PunybufType<'_>> {
		self.types.iter().map(|n| PunybufType(n))
	}
	pub fn commands(&self) -> impl Iterator<Item = PunybufCommand<'_>> {
		self.commands.iter().map(|n| PunybufCommand(n))
	}
}

impl PunybufDefinition {
	pub(crate) fn flatten_doc(&self, doc: String) -> String {
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

				let Some(mut whitespace_count) = remove_whitespace else { continue };

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
	pub(crate) fn flatten_reference(&mut self, refr: ValueReference) -> PBTypeRef {
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
			ValueReference::InlineDeclaration { symbol, name_span, decl, attributes, .. } => {
				self.flatten_flexible_decl(
					symbol.to_string(), name_span.clone(),
					"".to_string(),
					attributes,
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
	pub(crate) fn flatten_field(&mut self, field: Field) -> PBField {
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
			anonymous: field.anonymous,
			flags, attrs: field.attrs, doc: self.flatten_doc(field.doc)
		}
	}
	pub(crate) fn flatten_enum_variant(&mut self, ev: EnumVariant) -> PBEnumVariant {
		PBEnumVariant {
			name: ev.name, name_span: ev.name_span,
			discriminant: ev.discriminant,
			value: ev.value.map(|rf| self.flatten_reference(rf)),
			attrs: ev.attrs, doc: self.flatten_doc(ev.doc)
		}
	}
	pub(crate) fn flatten_value_enum_variant(&mut self, vev: ValueEnumVariant) -> PBEnumVariant {
		let name = vev.value.get_name().to_string();
		let name_span = vev.value.get_name_span().clone();
		PBEnumVariant {
			name, name_span,
			discriminant: vev.discriminant,
			value: Some(self.flatten_reference(vev.value)),
			attrs: vev.attrs, doc: self.flatten_doc(vev.doc)
		}
	}
	pub(crate) fn flatten_flexible_decl(
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

pub(crate) fn flatten(decls: Vec<Declaration>, includes_common: bool) -> Result<PunybufDefinition, PunybufError> {
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

				let command_id = if let Some(Some(override_id)) = decl.attrs.get("@id") {
					if let Some(_) = decl.attrs.get("@name") {
						return Err(pb_err!(
							decl.symbol_span,
							format!("cannot have both @id and @name on one command")
						));
					}
					override_id.parse::<u32>().map_err(|e|
						pb_err!(
							decl.symbol_span,
							format!("failed to parse @id({override_id}): {e}")
						)
					)?
				} else {
					let attr = decl.attrs.get("@name");
					PB_CRC.checksum(format!(
						"{}.{}",
						if let Some(Some(override_name)) = attr {
							override_name
						} else {
							&decl.symbol
						},
						layer
					).as_bytes())
				};

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

#[macro_export]
macro_rules! maybe_anon {
	($field:expr, $normal:literal, $anon:literal) => {{
		let f = &$field;
		if !f.anonymous {
			format!($normal, f.name)
		} else {
			format!($anon)
		}
	}};
	($field:expr, $normal:literal, $anon:literal, $($fmt:expr)*) => {{
		let f = &$field;
		if !f.anonymous {
			format!($normal, f.name, $($fmt,)+)
		} else {
			format!($anon, $($fmt,)+)
		}
	}}
}