use std::collections::HashMap;

use crate::{errors::{parser_err, pb_err, ExtendedErrorExplanation, InfoExplanation, InfoLevel, PunybufError}, flattener::{PBCommandArg, PBCommandDef, PBEnumVariant, PBField, PBFieldFlag, PBTypeDef, PBTypeRef, PunybufDefinition}, lexer::Span};

const COMMON_TYPES: [&str; 16] = [
	"Void",
	"U8",
	"U16",
	"U32",
	"U64",
	"I32",
	"I64",
	"UInt",
	"Array",
	"Bytes",
	"String",
	"Map",
	"KeyPair",
	"Done",
	"Boolean",
	"Optional",
];

enum FlagsAttrError<'a> {
	NoAttribute(&'a PBTypeDef),
	AliasGeneric {
		typedef: &'a PBTypeDef,
		ref_to_generic: (&'a String, &'a Span)
	},
	Other(PunybufError)
}

enum ReferenceDefinition<'a> {
	TopLevelDecl(&'a PBTypeDef),
	GenericArgument(Span)
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum ThingKind {
	Command,
	Type
}

pub enum Owner<'a> {
	TypeOwner(&'a PBTypeDef),
	CommandOwner(&'a PBCommandDef)
}

impl<'a> Owner<'a> {
	fn get_name(&self) -> (&String, &Span) {
		match self {
			Owner::CommandOwner(cmd) => (&cmd.name, &cmd.name_span),
			Owner::TypeOwner(tp) => tp.get_name()
		}
	}
	fn get_inline_owner(&self) -> &Option<(String, Span)> {
		match self {
			Owner::TypeOwner(tp) => tp.get_inline_owner(),
			Owner::CommandOwner(_) => &None
		}
	}
	fn get_attrs(&self) -> &HashMap<String, Option<String>> {
		match self {
			Owner::TypeOwner(tp) => tp.get_attrs(),
			Owner::CommandOwner(cmd) => &cmd.attrs
		}
	}
}

pub struct PunybufValidator<'pbd> {
	pub definition: &'pbd PunybufDefinition,
	context_generic_args: Vec<(&'pbd String, &'pbd Span)>
}

impl<'d> PunybufValidator<'d> {
	fn follow_to_flags_attr<'a>(
		&'a self, decl: &'a PBTypeDef,
		owner: &Owner, tries: usize
	) -> Result<usize, FlagsAttrError<'a>> {
		if tries >= 200 {
			return Err(FlagsAttrError::Other(
				pb_err!(
					owner.get_name().1,
					format!("reached limit for `@flags` evaluation for a field in this struct - \
					either you have ~200 aliases, which is cursed, ..."),
					ExtendedErrorExplanation::error_and(vec![
						InfoExplanation {
							span: decl.get_name().1.clone(),
							content: format!("...or `{}` is part of a cyclic alias", decl.get_name().0),
							level: InfoLevel::Error
						}
					])
				)
			));
		}
		match decl {
			PBTypeDef::Enum { attrs, .. } |
			PBTypeDef::Struct { attrs, ..} => {
				let Some(n) = attrs.get(&"@flags".to_string()) else {
					return Err(FlagsAttrError::NoAttribute(decl));
				};
				let Some(Ok(n)) = n.as_ref().map(|x| x.trim().parse::<usize>()) else {
					return Err(FlagsAttrError::Other(
						pb_err!(
							decl.get_name().1,
							format!("the `@flags` attribute on this type doesn't put a limit on how many flags are possible"),
							ExtendedErrorExplanation::error_and(vec![
								InfoExplanation {
									span: owner.get_name().1.clone(),
									content: format!("`{}` is mentioned here", decl.get_name().0),
									level: InfoLevel::Tip
								}
							])
						)
					));
				};
				Ok(n)
			}
			PBTypeDef::Alias { attrs, alias, generic_args, generic_span, .. } => {
				if let Some(n) = attrs.get(&"@flags".to_string()) {
					let Some(Ok(n)) = n.as_ref().map(|x| x.trim().parse::<usize>()) else {
						return Err(FlagsAttrError::Other(
							pb_err!(
								decl.get_name().1,
								format!("the `@flags` attribute on this type must put a limit on how many flags are possible"),
								ExtendedErrorExplanation::error_and(vec![
									InfoExplanation {
										span: owner.get_name().1.clone(),
										content: format!("`{}` is mentioned here", decl.get_name().0),
										level: InfoLevel::Tip
									}
								])
							)
						));
					};
					Ok(n)
				} else if attrs.contains_key(&"@builtin".to_string()) {
					return Err(FlagsAttrError::NoAttribute(decl));
				} else {
					let generics = generic_args.iter().map(|n| (n, generic_span)).collect();

					let def = self.validate_reference_void(
						alias,
						owner,
						Some(&generics)
					).map_err(|pbe| FlagsAttrError::Other(pbe))?;

					match def {
						ReferenceDefinition::GenericArgument(_) => {
							Err(FlagsAttrError::AliasGeneric {
								typedef: &decl,
								ref_to_generic: (&alias.reference, &alias.reference_span)
							})
						}
						ReferenceDefinition::TopLevelDecl(decl) => {
							self.follow_to_flags_attr(decl, owner, tries + 1)
						}
					}
				}
			}
		}
	}
	fn validate_reference(&self, refr: &PBTypeRef, owner: &Owner) -> Result<ReferenceDefinition<'_>, PunybufError> {
		if refr.reference == "Void" {
			return Err(parser_err!(
				refr.reference_span,
				"the reserved type `Void` is only allowed in command returns"
			));
		}
		self.validate_reference_void(refr, owner, None)
	}
	fn validate_reference_void(
		&self, refr: &PBTypeRef,
		owner: &Owner, override_generic_args: Option<&Vec<(&String, &Span)>>
	) -> Result<ReferenceDefinition<'_>, PunybufError> {
		let generic_args = override_generic_args.unwrap_or(&self.context_generic_args);

		if let Some(generic_ref) = generic_args.iter().find(|g| *g.0 == refr.reference) {
			if !refr.generics.is_empty() {
				return Err(pb_err!(
					refr.generic_span,
					format!("cannot provide generic parameters to a generic argument"),
					ExtendedErrorExplanation::error_and(vec![
						InfoExplanation {
							span: generic_ref.1.clone(),
							content: format!("generic arguments defined here"),
							level: InfoLevel::Info
						}
					])
				));
			}

			if let Some(decl) = self.definition.types.iter().find(|typ| *typ.get_name().0 == refr.reference) {
				match decl {
					PBTypeDef::Alias { .. } => {},
					PBTypeDef::Enum { inline_owner, .. } |
					PBTypeDef::Struct { inline_owner, .. } => 'block: {
						let Some(inline_owner) = inline_owner else {
							break 'block;
						};
						if inline_owner.0 != *owner.get_name().0 {
							break 'block;
						}

						return Err(pb_err!(
							refr.reference_span,
							format!("inline declaration of `{}` conflicts with a generic argument", refr.reference),
							ExtendedErrorExplanation::error_and(vec![
								InfoExplanation {
									span: generic_ref.1.clone(),
									content: format!("generic arguments, including `{}`, are defined here...", refr.reference),
									level: InfoLevel::Info
								},
								InfoExplanation {
									span: decl.get_name().1.clone(),
									content: format!("...but `{}` is also declared inline here", refr.reference),
									level: InfoLevel::Info
								}
							])
						));
					}
				}
			}
			return Ok(ReferenceDefinition::GenericArgument(generic_ref.1.clone()));
		}

		match self.definition.types.iter().find(|typ| *typ.get_name().0 == refr.reference) {
			Some(decl) => {
				match decl {
					PBTypeDef::Alias { .. } => {
						// aliases cannot be declared inline
					}
					PBTypeDef::Enum { inline_owner, name_span, .. } |
					PBTypeDef::Struct { inline_owner, name_span, .. } => {
						match inline_owner {
							Some((valid_owner, valid_owner_span)) => {
								if valid_owner != owner.get_name().0 {
									let mut explanation = vec![
										InfoExplanation {
											span: valid_owner_span.clone(),
											content: format!("inside `{valid_owner}`..."),
											level: InfoLevel::Info
										},
										InfoExplanation {
											span: name_span.clone(),
											content: format!("...`{}` is declared inline...", refr.reference),
											level: InfoLevel::Info
										},
										InfoExplanation {
											span: owner.get_name().1.clone(),
											content: format!("...but inside `{}`...", owner.get_name().0),
											level: InfoLevel::Info
										},
										InfoExplanation {
											span: refr.reference_span.clone(),
											content: format!("...`{}` is referenced, outside of `{valid_owner}`", refr.reference),
											level: InfoLevel::Error
										}
									];

									match owner.get_inline_owner() {
										None => {}
										Some(owner_of_owner) => if owner_of_owner.0 == refr.reference ||
											owner_of_owner.0 == *valid_owner
										{
											explanation.push(InfoExplanation {
												span: owner_of_owner.1.clone(),
												content: format!("info: even though inside `{}`...", owner_of_owner.0),
												level: InfoLevel::Info
											});
											explanation.push(InfoExplanation {
												span: owner.get_name().1.clone(),
												content: format!(
													"...`{}` is declared inline...",
													owner.get_name().0
												),
												level: InfoLevel::Info
											});
											explanation.push(InfoExplanation {
												span: refr.reference_span.clone(),
												content: format!(
													"...you may reference `{}` only directly from inside `{valid_owner}`, \
													not from `{}`",
													refr.reference, owner.get_name().0
												),
												level: InfoLevel::Error
											});
											explanation.push(InfoExplanation {
												span: refr.reference_span.clone(),
												content: format!("also, `{}` is a cyclic type, so be careful!", refr.reference),
												level: InfoLevel::Warning
											});
										}
									}

									return Err(pb_err!(
										refr.reference_span,
										format!("type `{}` is inline and cannot be referenced outside `{valid_owner}`", refr.reference),
										ExtendedErrorExplanation::custom(explanation)
									));
								}
							}
							None => {}
						}
					},
				}
				
				let (decl_generic_args, decl_generic_span) = decl.get_generics();
				if decl_generic_args.len() > refr.generics.len() {
					let not_provided = decl_generic_args.split_at(refr.generics.len()).1;
					return Err(pb_err!(
						if refr.generic_span == Span::impossible() { refr.reference_span.clone() }
						else { refr.generic_span.clone() },

						format!(
							"type `{}` takes {} generic arguments, but only {} were provided",
							refr.reference, decl_generic_args.len(), refr.generics.len()
						),

						ExtendedErrorExplanation::custom(vec![
							InfoExplanation {
								span: decl_generic_span.clone(),
								content: format!("generic arguments for `{}` are defined here", refr.reference),
								level: InfoLevel::Info
							},
							if refr.generic_span == Span::impossible() {
								InfoExplanation {
									span: refr.reference_span.clone(),
									content: format!("no generic parameters (`< ... >`) provided at all"),
									level: InfoLevel::Error
								}
							} else {
								InfoExplanation {
									span: refr.generic_span.clone(),
									content: format!("missing generic parameters: `{}`", not_provided.join("`, `")),
									level: InfoLevel::Error
								}
							},
						])
					));
				}
				if decl_generic_args.len() < refr.generics.len() {
					return Err(pb_err!(
						if refr.generic_span == Span::impossible() { refr.reference_span.clone() }
						else { refr.generic_span.clone() },
						format!(
							"type `{}` takes only {} generic arguments, but {} were provided",
							refr.reference, decl_generic_args.len(), refr.generics.len()
						),
						ExtendedErrorExplanation::error_and(vec![
							if *decl_generic_span == Span::impossible() {
								InfoExplanation {
									span: decl.get_name().1.clone(),
									content: format!("`{}` takes no generics (`< ... >`)", refr.reference),
									level: InfoLevel::Info
								}
							} else {
								InfoExplanation {
									span: decl_generic_span.clone(),
									content: format!("generic arguments for `{}` are defined here", refr.reference),
									level: InfoLevel::Info
								}
							},
						])
					));
				}

				for x in &refr.generics {
					self.validate_reference_void(x, owner, override_generic_args)?;
				}

				Ok(ReferenceDefinition::TopLevelDecl(decl))
			},
			None => {
				if COMMON_TYPES.iter().find(|x| x == &&refr.reference).is_some() {
					return Err(pb_err!(
						refr.reference_span,
						format!("cannot find type `{}` in scope, perhaps you forgot to `include common`?", refr.reference)
					));
				}
				Err(pb_err!(
					refr.reference_span,
					format!("cannot find type `{}` in scope", refr.reference)
				))
			}
		}
	}
	pub fn validate_generic_args(args: &Vec<String>, span: &Span) -> Result<(), PunybufError> {
		let mut declared_args: Vec<&String> = vec![];
		for ga in args {
			if declared_args.contains(&ga) {
				return Err(pb_err!(
					span,
					format!("generic argument `{ga}` defined twice")
				));
			}
			declared_args.push(ga);
		}
		Ok(())
	}
	pub fn validate_flags(&self, owner: &Owner, flags: &Vec<PBFieldFlag>) -> Result<(), PunybufError> {
		let is_sealed = owner.get_attrs().contains_key("@sealed");
		let mut extension_begin = None::<(&String, &Span)>;

		let mut seen_names: Vec<(&String, &Span)> = vec![];
		for flag in flags {
			if let Some(dupe) = seen_names.iter().find(|n| *n.0 == flag.name) {
				return Err(pb_err!(
					flag.name_span,
					format!("flag `{}` defined twice", flag.name),
					ExtendedErrorExplanation::custom(vec![
						InfoExplanation {
							span: dupe.1.clone(),
							content: format!("`{}` defined here first", dupe.0),
							level: InfoLevel::Info
						},
						InfoExplanation {
							span: flag.name_span.clone(),
							content: format!("`{}` defined here again", dupe.0),
							level: InfoLevel::Error
						},
					])
				));
			}
			seen_names.push((&flag.name, &flag.name_span));

			if is_sealed && flag.attrs.contains_key("@extension") {
				return Err(pb_err!(
					flag.name_span,
					format!("tried to extend a `@sealed` struct"),
					ExtendedErrorExplanation::custom(vec![
						InfoExplanation {
							span: owner.get_name().1.clone(),
							content: format!("`{}` marked as `@sealed` here...", owner.get_name().0),
							level: InfoLevel::Info
						},
						InfoExplanation {
							span: owner.get_name().1.clone(),
							content: format!("...but contains an `@extension` flag here"),
							level: InfoLevel::Error
						},
						InfoExplanation {
							span: owner.get_name().1.clone(),
							content: format!("`@extension` and `@sealed` are incompatible"),
							level: InfoLevel::Info
						}
					])
				));
			}

			if flag.attrs.contains_key("@extension") {
				extension_begin = Some((&flag.name, &flag.name_span));
			} else if let Some((_, ext_span)) = extension_begin {
				return Err(pb_err!(
					flag.name_span,
					format!("a regular flag cannot follow an `@extension` flag"),
					ExtendedErrorExplanation::error_and(vec![
						InfoExplanation {
							span: ext_span.clone(),
							content: format!("this `@extension` flag is before `{}`", flag.name),
							level: InfoLevel::Info
						}
					])
				));
			}

			if let Some(refr) = &flag.value {
				self.validate_reference(refr, owner)?;
			}
		}
		Ok(())
	}
	pub fn validate_struct(&mut self, owner: &Owner, fields: &Vec<PBField>) -> Result<(), PunybufError> {
		let mut seen_names: Vec<(&String, &Span)> = vec![];
		for field in fields {
			if field.attrs.contains_key("@extension") {
				return Err(pb_err!(
					field.name_span,
					format!("`@extension`s are only allowed to be defined on flags")
				));
			}
			if let Some(already_decl) = seen_names.iter().find(|n| *n.0 == field.name) {
				return Err(pb_err!(
					already_decl.1,
					format!("field `{}` defined twice", already_decl.0),
					ExtendedErrorExplanation::custom(vec![
						InfoExplanation {
							span: already_decl.1.clone(),
							content: format!("`{}` defined here first", already_decl.0),
							level: InfoLevel::Info
						},
						InfoExplanation {
							span: field.name_span.clone(),
							content: format!("`{}` defined here again", already_decl.0),
							level: InfoLevel::Error
						},
					])
				));
			}
			seen_names.push((&field.name, &field.name_span));

			let field_ref_def = self.validate_reference(&field.value, owner)?;
			if let Some(flags) = &field.flags {
				let field_ref_decl = match field_ref_def {
					ReferenceDefinition::TopLevelDecl(x) => x,
					ReferenceDefinition::GenericArgument(span) => {
						return Err(pb_err!(
							field.value.reference_span,
							format!("flag fields' types must be marked `@flags`, \
							but `{}` is a generic argument and cannot be constrained", field.value.reference),
							ExtendedErrorExplanation::error_and(vec![
								InfoExplanation {
									span: span.clone(),
									content: format!("generic arguments for `{}` defined here", owner.get_name().0),
									level: InfoLevel::Info,
								}
							])
						));
					}
				};
				let decl_span = match field_ref_decl {
					PBTypeDef::Alias { name_span, .. } |
					PBTypeDef::Struct { name_span, .. } |
					PBTypeDef::Enum { name_span, .. } => name_span,
				};
				match self.follow_to_flags_attr(field_ref_decl, owner, 0) {
					Ok(max_amount) => if flags.len() > max_amount {
						return Err(pb_err!(
							field.name_span,
							format!("too many flags ({}); maximum amount of flags for `{}` is {max_amount}", flags.len(), field.value.reference),
							ExtendedErrorExplanation::error_and(vec![
								InfoExplanation {
									span: field.value.reference_span.clone(),
									content: format!("the maximum amount of flags is bounded by type `{}`", field.value.reference),
									level: InfoLevel::Info
								}
							])
						));
					}
					Err(FlagsAttrError::Other(pbe)) => return Err(pbe),
					Err(FlagsAttrError::NoAttribute(decl)) => {
						let mut after_error: Vec<InfoExplanation> = vec![
							InfoExplanation {
								span: decl_span.clone(),
								content: format!("`{}` is defined here, without the `@flags` attribute", field.value.reference),
								level: InfoLevel::Info
							}
						];
						if *decl.get_name().0 != field.value.reference {
							after_error.push(
								InfoExplanation {
									span: decl.get_name().1.clone(),
									content: format!("...this alias leads to `{}`, also without the `@flags` attribute", decl.get_name().0),
									level: InfoLevel::Info
								}
							);
						}
						return Err(pb_err!(
							field.value.reference_span,
							format!("flag fields' types must be marked `@flags`, `{}` is not", field.value.reference),
							ExtendedErrorExplanation::error_and(after_error)
						))
					}
					Err(FlagsAttrError::AliasGeneric { typedef, ref_to_generic }) => {
						let mut after_error: Vec<InfoExplanation> = vec![
							InfoExplanation {
								span: decl_span.clone(),
								content: format!("`{}` is defined here, without the `@flags` attribute...", field.value.reference),
								level: InfoLevel::Info
							}
						];
						if *typedef.get_name().0 != field.value.reference {
							after_error.push(
								InfoExplanation {
									span: typedef.get_name().1.clone(),
									content: format!("...this alias leads to `{}`, also without the `@flags` attribute...", typedef.get_name().0),
									level: InfoLevel::Info
								}
							);
						}
						after_error.push(
							InfoExplanation {
								span: typedef.get_generics().1.clone(),
								content: format!("...which defines its generic arguments here..."),
								level: InfoLevel::Info
							}
						);
						after_error.push(
							InfoExplanation {
								span: ref_to_generic.1.clone(),
								content: format!("...and later aliases to `{}`, which cannot be constrained as `@flags`", ref_to_generic.0),
								level: InfoLevel::Info
							}
						);
						return Err(pb_err!(
							field.value.reference_span.extend(&field.value.generic_span),
							format!("flag fields' types must be marked `@flags`, cannot verify if `{}< ... >` is", field.value.reference),
							ExtendedErrorExplanation::error_and(after_error)
						))
					},
				}
				self.validate_flags(owner, flags)?;
			}
		}
		return Ok(());
	}
	pub fn validate_enum(&mut self, owner: &Owner, variants: &Vec<PBEnumVariant>) -> Result<(), PunybufError> {
		let mut default_variant_present = false;
		let mut extension_discriminant = None::<u8>;

		let mut seen_names: Vec<(&String, &Span)> = vec![];
		for variant in variants {
			if let Some(already_decl) = seen_names.iter().find(|n| *n.0 == variant.name) {
				return Err(pb_err!(
					variant.name_span,
					format!("enum variant `{}` defined twice", already_decl.0),
					ExtendedErrorExplanation::custom(vec![
						InfoExplanation {
							span: already_decl.1.clone(),
							content: format!("`{}` defined here first", already_decl.0),
							level: InfoLevel::Info
						},
						InfoExplanation {
							span: variant.name_span.clone(),
							content: format!("`{}` defined here again", already_decl.0),
							level: InfoLevel::Error
						},
					])
				));
			}
			seen_names.push((&variant.name, &variant.name_span));

			// TODO: validate the discriminant
			// (right now, you can't set your own so it's fine)

			if variant.attrs.contains_key("@default") {
				if variant.attrs.contains_key("@extension") {
					return Err(pb_err!(
						variant.name_span,
						format!("an enum variant cannot both be `@default` and an `@extension`")
					));
				}
				if let Some(val) = &variant.value {
					return Err(pb_err!(
						variant.name_span,
						format!("a `@default` enum variant cannot have an associated type"),
						ExtendedErrorExplanation::error_and(vec![
							InfoExplanation {
								span: val.reference_span.clone(),
								content: format!("the associated type is defined here"),
								level: InfoLevel::Info
							}
						])
					));
				}
				default_variant_present = true;
			}

			if variant.attrs.contains_key("@extension") {
				// right now, since all enum variants are
				// sequential, a @default key must always come before
				// @extensions
				if !default_variant_present {
					return Err(pb_err!(
						variant.name_span,
						format!("an `@extension` variant cannot be defined without a `@default` variant present")
					));
				};
				extension_discriminant = Some(variant.discriminant);

			} else if let Some(extension_discriminant) = extension_discriminant {
				if extension_discriminant < variant.discriminant {
					return Err(pb_err!(
						variant.name_span,
						format!("a regular enum variant cannot follow an `@extension` one")
					));
				}
			}

			if let Some(value) = &variant.value {
				self.validate_reference(value, owner)?;
			}
		};
		Ok(())
	}
	pub fn validate_type(&mut self, tp: &'d PBTypeDef) -> Result<(), PunybufError> {
		let (attrs, generic_args, generic_span) = match tp {
			PBTypeDef::Alias { attrs, generic_args, generic_span, .. } |
			PBTypeDef::Enum { attrs, generic_args, generic_span, .. } |
			PBTypeDef::Struct { attrs, generic_args, generic_span, .. }
				=> (attrs, generic_args, generic_span)
		};
		Self::validate_generic_args(generic_args, generic_span)?;
		if attrs.contains_key("@builtin") {
			// Builtins aren't checked because whatever you write inside them
			// doesn't matter as they're meant to be constructed outside
			// of the punybuf realm
			return Ok(());
		}

		self.context_generic_args = generic_args.iter().map(|n| (n, generic_span)).collect();

		let mut is_alias = false;

		match tp {
			PBTypeDef::Alias { alias, .. } => {
				self.validate_reference(alias, &Owner::TypeOwner(tp))?;
				is_alias = true;
			}
			PBTypeDef::Enum { variants, .. } => {
				self.validate_enum(&Owner::TypeOwner(tp), variants)?;
			}
			PBTypeDef::Struct { fields, .. } => {
				self.validate_struct(&Owner::TypeOwner(tp), fields)?;
			}
		}

		if tp.get_attrs().contains_key("@resolve") && !is_alias {
			return Err(pb_err!(
				tp.get_name().1,
				format!("only aliases may be marked as `@resolve`")
			));
		}

		self.context_generic_args = vec![];
		Ok(())
	}
	pub fn validate_command(&mut self, cmd: &'d PBCommandDef) -> Result<(), PunybufError> {
		match &cmd.argument {
			PBCommandArg::Struct { fields } => {
				self.validate_struct(&Owner::CommandOwner(cmd), fields)?;
			}
			PBCommandArg::Ref(rf) => {
				self.validate_reference(rf, &Owner::CommandOwner(cmd))?;
			}
			PBCommandArg::None => {}
		};

		self.validate_reference_void(&cmd.ret, &Owner::CommandOwner(cmd), None)?;

		if cmd.ret.reference == "Void" && cmd.err.len() > 0 {
			return Err(pb_err!(
				cmd.err_span,
				format!("commands that return `Void` cannot respond with errors"),
				ExtendedErrorExplanation::error_and(vec![
					InfoExplanation {
						span: cmd.ret.reference_span.clone(),
						content: format!("`{}` is said to return `Void` here", cmd.name),
						level: InfoLevel::Info
					}
				])
			));
		}
		self.validate_enum(&Owner::CommandOwner(cmd), &cmd.err)?;

		Ok(())
	}
	/// Validates the Punybuf definition further, catching things like
	/// re-declarations, references to inline declarations, and stuff like that
	/// 
	/// Known issue: does not catch self-referential types.
	pub fn validate(&mut self) -> Result<(), PunybufError> {
		let mut declared_things: Vec<(&String, &u32, &Span, ThingKind)> = vec![];
		for tp in &self.definition.types {
			if let Some(already_decl) = declared_things.iter().find(|x| x.0 == tp.get_name().0 && x.1 == tp.get_layer()) {
				return Err(pb_err!(
					already_decl.2,
					format!("`{}` declared twice", already_decl.0),
					ExtendedErrorExplanation::custom(vec![
						InfoExplanation {
							span: already_decl.2.clone(),
							content: format!("`{}` declared here first", already_decl.0),
							level: InfoLevel::Info
						},
						InfoExplanation {
							span: tp.get_name().1.clone(),
							content: format!("`{}` declared here again", already_decl.0),
							level: InfoLevel::Error
						},
					])
				));
				// checking for kinds of things doesn't matter here since at that point there can't be any commands in already_decl
			}
			let attrs = tp.get_attrs();
			let name = tp.get_name();
			if name.0 == "Void" && !attrs.contains_key("@void") {
				return Err(parser_err!(name.1, "cannot declare a reserved type `Void`, unless the `@void` attribute is present"));
			}
			declared_things.push((name.0, tp.get_layer(), name.1, ThingKind::Type));
			if name.0 != "Void" {
				self.validate_type(tp)?;
			}
		}

		let mut just_in_case_seen_ids = HashMap::<u32, (&String, &u32, &Span)>::new();
		for cmd in &self.definition.commands {
			if let Some(already_decl) = declared_things
				.iter()
				.find(|x| x.0 == &cmd.name && (x.1 == &cmd.layer || x.3 != ThingKind::Command))
			{
				if already_decl.1 == &cmd.layer {
					return Err(pb_err!(
						already_decl.2,
						format!("`{}` declared twice", already_decl.0),
						ExtendedErrorExplanation::custom(vec![
							InfoExplanation {
								span: already_decl.2.clone(),
								content: format!("`{}` declared here first", already_decl.0),
								level: InfoLevel::Info
							},
							InfoExplanation {
								span: cmd.name_span.clone(),
								content: format!("`{}` declared here again", already_decl.0),
								level: InfoLevel::Error
							},
						])
					));

				} else if already_decl.3 != ThingKind::Command {
					return Err(pb_err!(
						already_decl.2,
						format!("invalid redeclaration of `{}`; even in different layers, types can't become commands (and vice versa)", already_decl.0),
						ExtendedErrorExplanation::custom(vec![
							InfoExplanation {
								span: already_decl.2.clone(),
								content: format!("`{}` declared here, in layer {}, as a type", already_decl.0, already_decl.1),
								level: InfoLevel::Error
							},
							InfoExplanation {
								span: cmd.name_span.clone(),
								content: format!("`{}` declared here, in layer {}, as a command", already_decl.0, already_decl.1),
								level: InfoLevel::Error
							},
						])
					));
				}
			}
			if cmd.name == "Void" {
				return Err(parser_err!(cmd.name_span, "cannot declare a command with the reserved name `Void`"));
			}
			declared_things.push((&cmd.name, &cmd.layer, &cmd.name_span, ThingKind::Command));
			self.validate_command(cmd)?;

			if let Some((other_name, other_layer, other_span)) = just_in_case_seen_ids.remove(&cmd.command_id) {
				// doesn't check the crc32 after resolution, but bro cmon
				return Err(pb_err!(
					cmd.name_span,
					"by some miracle, two commands produce the same crc32 checksum, and thus, have the same command ID".to_string(),
					ExtendedErrorExplanation::custom(vec![
						InfoExplanation {
							span: other_span.clone(),
							content: format!("command {other_name} of {other_layer}: `crc32(\"{other_name}.{other_layer}\") -> {}`", cmd.command_id),
							level: InfoLevel::Info
						},
						InfoExplanation {
							span: other_span.clone(),
							content: format!("command {name} of {layer}: `crc32(\"{name}.{layer}\") -> {}`", cmd.command_id, name=cmd.name, layer=cmd.layer),
							level: InfoLevel::Error
						}
					])
				));
			}
			just_in_case_seen_ids.insert(cmd.command_id, (&cmd.name, &cmd.layer, &cmd.name_span));
		}
		Ok(())
	}
}

impl PunybufDefinition {
	pub fn as_validator(&self) -> PunybufValidator<'_> {
		PunybufValidator { definition: self, context_generic_args: vec![] }
	}
	pub fn validate(&self) -> Result<(), PunybufError> {
		self.as_validator().validate()
	}
}