use std::collections::HashMap;
use std::{iter::Peekable, slice::Iter, vec};

use crate::errors::{parser_err, pb_err, ExtendedErrorExplanation, InfoExplanation, InfoLevel, PunybufError};

use crate::lexer::{Span, Token, TokenData};

#[derive(Debug)]
pub enum ValueReference {
	Reference {
		name: String,
		name_span: Span,
		generics: Vec<ValueReference>,
		generic_span: Span,
	},
	InlineDeclaration {
		symbol: String,
		name_span: Span,
		decl: FlexibleDeclarationValue,
		decl_span: Span,
	}
}
impl ValueReference {
	pub fn get_name(&self) -> &String {
		match &self {
			ValueReference::InlineDeclaration { symbol, decl: _, name_span: _, decl_span: _ } => symbol,
			ValueReference::Reference { name, generics: _, name_span: _, generic_span: _ } => name
		}
	}
	pub fn get_name_span(&self) -> &Span {
		match &self {
			ValueReference::InlineDeclaration { symbol: _, decl: _, name_span, decl_span: _ } => name_span,
			ValueReference::Reference { name: _, generics: _, name_span, generic_span: _ } => name_span
		}
	}
}

#[derive(Debug)]
pub struct FieldFlag {
	pub name: String,
	pub name_span: Span,
	pub value: Option<ValueReference>,
	pub attrs: HashMap<String, Option<String>>,
	pub doc: String,
}

#[derive(Debug)]
pub struct Field {
	pub name: String,
	pub name_span: Span,
	pub value: ValueReference,
	pub flags: Option<Vec<FieldFlag>>,
	pub attrs: HashMap<String, Option<String>>,
	pub doc: String,
}

#[derive(Debug)]
pub struct EnumVariant {
	pub name: String,
	pub name_span: Span,
	pub discriminant: u8,
	pub value: Option<ValueReference>,
	pub attrs: HashMap<String, Option<String>>,
	pub doc: String,
}

#[derive(Debug)]
pub struct ValueEnumVariant {
	pub discriminant: u8,
	pub value: ValueReference,
	pub attrs: HashMap<String, Option<String>>,
	pub doc: String,
}

#[derive(Debug)]
pub enum FlexibleDeclarationValue {
	StructDeclaration {
		inline: bool,
		layer: u32,
		fields: Vec<Field>,
	},
	EnumDeclaration {
		inline: bool,
		layer: u32,
		variants: Vec<EnumVariant>
	},
	ValueEnumDeclaration {
		inline: bool,
		layer: u32,
		variants: Vec<ValueEnumVariant>
	},
}

#[derive(Debug)]
pub enum CommandArgument {
	None,
	Reference(ValueReference),
	Struct {
		fields: Vec<Field>
	}
}

#[derive(Debug)]
pub enum DeclarationValue {
	Flexible {
		val: FlexibleDeclarationValue,
		val_span: Span,
		generic_args: Vec<String>,
		generic_span: Span,
	},
	AliasDeclaration {
		generic_args: Vec<String>,
		generic_span: Span,
		layer: u32,
		alias: Box<ValueReference>,
	},
	CommandDeclaration {
		argument: CommandArgument,
		argument_span: Span,
		layer: u32,
		ret: Box<ValueReference>,
		/// Only enums allowed
		err: Option<Box<FlexibleDeclarationValue>>,
		err_span: Span,
	},
}

#[derive(Debug)]
pub struct Declaration {
	pub symbol: String,
	pub symbol_span: Span,
	pub value: DeclarationValue,
	pub attrs: HashMap<String, Option<String>>,
	pub doc: String
}

pub struct Parser<'parser> {
	peekable: Peekable<Iter<'parser, Token>>,
}

impl<'parser> Parser<'parser> {
	pub fn new(tokens: &'parser Vec<Token>) -> Self {
		Self {
			peekable: tokens.iter().peekable()
		}
	}
	pub fn parse(&mut self) -> Result<Vec<Declaration>, PunybufError> {
		let mut decls = Vec::new();
		let mut nextdoc: Option<(&str, &Span)> = None;
		let mut next_attrs = HashMap::new();

		let mut layer = 0u32;

		while let Some(tk) = self.peekable.next() {
			match &tk.data {
				TokenData::Attribute(attr, val) => {
					next_attrs.insert(attr.clone(), val.clone());
				}
				TokenData::Docs(doc) => {
					if let Some((_, first_span)) = nextdoc {
						return Err(pb_err!(
							tk.span,
							format!("documentation defined twice"),
							ExtendedErrorExplanation::custom(vec![
								InfoExplanation {
									span: first_span.clone(),
									content: format!("documentation defined here first"),
									level: InfoLevel::Info
								},
								InfoExplanation {
									span: tk.span.clone(),
									content: format!("...then defined here again"),
									level: InfoLevel::Error
								},
							])
						));
					}
					nextdoc = Some((doc, &tk.span));
				}
				TokenData::Symbol(name) => {
					let mut equals_or_colon = self.peekable.next().ok_or(parser_err!(tk.span, "unexpected EOF"))?;
					let mut generic_arguments = Vec::new();
					let mut generic_span = Span::impossible();

					match &equals_or_colon.data {
						TokenData::AngleBrackets(inner) => {
							let mut inner_peekable = inner.iter().peekable();
							generic_span = equals_or_colon.span.clone();

							while let Some(token) = inner_peekable.next() {
								match &token.data {
									TokenData::Symbol(generic) => {
										let next = inner_peekable.next();
										match next {
											None => {},
											Some(next) => {
												if next.data != TokenData::Comma {
													return Err(parser_err!(next.span, "generic arguments must be separated by a comma (`,`)"));
												}
											}
										}
										generic_arguments.push(generic.to_string());
									}
									_ => {
										return Err(parser_err!(token.span, "expected an identifier, got `{token}`"));
									}
								}
							}
							equals_or_colon = self.peekable.next().ok_or(parser_err!(tk.span, "unexpected EOF"))?;
						}
						_ => {}
					}

					let value: DeclarationValue;
					match equals_or_colon.data {
						TokenData::Equals => {
							let next = self.peekable.peek();
							match next {
								Some(Token { data: TokenData::Symbol(_), span: _ }) => {
									let refr = Parser::parse_reference(&mut self.peekable, &equals_or_colon.span, layer)?;
									value = DeclarationValue::AliasDeclaration {
										generic_args: generic_arguments,
										generic_span, layer,
										alias: Box::new(refr)
									};
								}
								_ => {
									let (flex, val_span) = Parser::parse_decl(
										&mut self.peekable, &equals_or_colon.span,
										false, false, layer,
									)?;
									value = DeclarationValue::Flexible {
										val: flex,
										val_span,
										generic_span,
										generic_args: generic_arguments
									};
								}
							}
						},
						TokenData::Colon => {
							if generic_span != Span::impossible() {
								return Err(parser_err!(generic_span, "commands may not be generic"));
							}

							let next = self.peekable.peek().ok_or(parser_err!(equals_or_colon.span, "unexpected EOF"))?;
							let argument_span = next.span.clone();

							let variable_because_rust_sucks = parser_err!(
								// jk, rust is cool but just annoying as hell sometimes <3
								next.span,
								"expected an `->` for the command return type, got EOF; if the command doesn't return anything, use `Void`"
							);

							let argument = match &next.data {
								TokenData::Symbol(_) => {
									let refr = Parser::parse_reference(&mut self.peekable, &equals_or_colon.span, layer)?;
									CommandArgument::Reference(refr)
								}
								TokenData::CurlyBraces(inside) => {
									if inside.is_empty() {
										self.peekable.next();
										CommandArgument::None
									} else {
										let decl = Parser::parse_decl(
											&mut self.peekable, &equals_or_colon.span,
											false, false, layer
										)?;
										match decl {
											(FlexibleDeclarationValue::StructDeclaration { inline: _, fields, .. }, _span) => {
												CommandArgument::Struct { fields }
											},
											_ => {
												return Err(parser_err!(decl.1, "only struct definitions (`{{ ... }}`) and references are allowed as command arguments"));
											}
										}
									}
								}
								TokenData::Parentheses(inside) => {
									let next = self.peekable.next().unwrap(); // Safe, beacuse `next` was peeked
									if !inside.is_empty() {
										return Err(
											pb_err!(
												next.span,
												format!("expected either `{{ ... }}`, empty `()`, or an identifier, got {next}"),
												ExtendedErrorExplanation::error_and(vec![
													InfoExplanation {
														span: next.span.clone(),
														content: format!("if this is intended to be a value-enum declaration, put the name of the value-enum before the parentheses"),
														level: InfoLevel::Tip
													}
												])
											));
									}
									CommandArgument::None
								}
								_ => {
									return Err(parser_err!(next.span, "expected either `{{ ... }}`, empty `()`, or an identifier, got {next}"));
								}
							};

							let arrow = self.peekable.next().ok_or(variable_because_rust_sucks)?;
							if arrow.data != TokenData::Arrow {
								return Err(parser_err!(arrow.span, "expected an `->` for the command return type, got `{arrow}`; if the command doesn't return anything, use `Void`"));
							}

							let ret = Parser::parse_reference(&mut self.peekable, &arrow.span, layer)?;

							let mut err = None;
							let mut err_span = Span::impossible();

							match self.peekable.peek() {
								Some(Token { data: TokenData::Bang, span }) => {
									self.peekable.next();

									let (decl, decl_span) = Parser::parse_decl(
										&mut self.peekable, span,
										false, true, layer
									)?;

									match decl {
										FlexibleDeclarationValue::StructDeclaration { .. } => {
											return Err(PunybufError {
												span: span.extend(&decl_span),
												error: format!("all errors must be enums (or value-enums)"),
												explanation: Some(ExtendedErrorExplanation::error_and(vec![
													InfoExplanation {
														span: decl_span,
														content: format!("give a name to this struct and declare it inline as part of a value-enum, like `!(ErrorName {{ ... }})`"),
														level: InfoLevel::Tip
													}
												]))
											});
										}
										_ => {}
									}

									err = Some(Box::new(decl));
									err_span = span.extend(&decl_span);
								}
								_ => {}
							}

							value = DeclarationValue::CommandDeclaration {
								argument, argument_span, layer,
								ret: Box::new(ret),
								err, err_span
							}
						},
						_ => {
							return Err(parser_err!(
								equals_or_colon.span,
								"unexpected token `{}`; in a declaration, an identifier should be followed by either `=` or `:`",
								equals_or_colon
							));
						}
					}
					decls.push(Declaration {
						symbol: name.to_string(),
						symbol_span: tk.span.clone(),
						value,
						attrs: next_attrs,
						doc: nextdoc.unwrap_or(("", &Span::impossible())).0.to_string()
					});
					nextdoc = None;
					next_attrs = HashMap::new();
				},
				TokenData::LayerKeyword => {
					match self.peekable.next() {
						Some(Token { data: TokenData::Numeric(layer_decl), span }) => {
							layer = *layer_decl;
							match self.peekable.next() {
								Some(Token { data: TokenData::Colon, span: _ }) => {},
								Some(t) => {
									return Err(parser_err!(t.span, "expected a colon (`:`) after the layer declaration, got `{t}`"));
								}
								None => {
									return Err(parser_err!(tk.span.extend(&span), "expected a colon (`:`) after the layer declaration"));
								}
							}
						}
						Some(t) => {
							return Err(parser_err!(t.span, "expected a number for the layer declaration, got `{t}`"));
						}
						_ => {
							return Err(parser_err!(tk.span, "expected a number for the layer declaration"));
						}
					}
				}
				_ => {
					return Err(parser_err!(tk.span, "expected `#[ ... ]`, a layer declaration or an identifier, got `{tk}`"));
				}
			}
		}

		Ok(decls)
	}

	fn parse_generics(tokens: &Vec<Token>, layer: u32) -> Result<Vec<ValueReference>, PunybufError> {
		let mut gen = Vec::new();
		let mut peekable = tokens.iter().peekable();

		while let Some(_) = peekable.peek() {
			let refr = Parser::parse_reference(&mut peekable, &Span::impossible(), layer)?;
			let comma = peekable.next();
			match comma {
				Some(Token { data: TokenData::Comma, span: _ }) => {}
				Some(tk) => {
					return Err(parser_err!(tk.span, "unexpected token `{tk}`; generic parameters must be separated by a comma (`,`)"));
				}
				_ => {}
			}
			gen.push(refr);
		}

		Ok(gen)
	}

	/// Consumes the next token, which is expected to be (), {} or []
	fn parse_decl(
		peekable: &mut Peekable<Iter<Token>>, before_decl: &Span,
		is_inline: bool, start_at_one: bool, layer: u32
	) -> Result<(FlexibleDeclarationValue, Span), PunybufError> {
		let brackets = peekable.next().ok_or(parser_err!(before_decl, "this situation should be impossible, lol"))?;
		match &brackets.data {
			TokenData::CurlyBraces(inside) => Ok((
				Parser::parse_struct_decl(
					inside,
					if is_inline { Some(before_decl) } else { None },
					layer
				)?,
				brackets.span.clone()
			)),
			TokenData::SquareBrackets(inside) => Ok((
				Parser::parse_enum_decl(inside, start_at_one, layer)?,
				brackets.span.clone()
			)),
			TokenData::Parentheses(inside) => Ok((
				Parser::parse_value_enum_decl(inside, start_at_one, layer)?,
				brackets.span.clone()
			)),
			_ => {
				Err(parser_err!(brackets.span, "expected one of `()`, `{{}}` or `[]`, got `{brackets}`"))
			}
		}
	}

	fn parse_struct_decl(tokens: &Vec<Token>, before_inline_decl: Option<&Span>, layer: u32) -> Result<FlexibleDeclarationValue, PunybufError> {
		let mut fields = vec![];
		let mut peekable = tokens.iter().peekable();

		let mut next_doc: Option<&str> = None;
		let mut next_attrs = HashMap::new();
		while let Some(token) = peekable.next() {
			match &token.data {
				TokenData::Attribute(attr, val) => {
					next_attrs.insert(attr.clone(), val.clone());
				}
				TokenData::Docs(doc) => {
					if let Some(_) = next_doc {
						return Err(parser_err!(token.span, "documentaion description defined twice"));
					}
					next_doc = Some(doc);
				}
				TokenData::Symbol(field_name) => {
					let next = peekable.next().ok_or(parser_err!(token.span, "expected a `:`, found nothing"))?;
					match next.data {
						TokenData::Colon => {},
						TokenData::Question => {
							if let Some(before_inline_decl) = before_inline_decl {
								return Err(PunybufError {
									span: next.span.clone(),
									error: "expected a `:` after the field name, got `?`".to_string(),
									explanation: Some(
										ExtendedErrorExplanation::error_and(vec![
											InfoExplanation {
												content: format!("if this is inteded to be a flag, put a dot (`.`) after this inline declaration's identifier"),
												span: before_inline_decl.clone(),
												level: InfoLevel::Tip,
											}
										])
									)
								});
							} else {
								return Err(parser_err!(next.span, "expected a `:` after the field name, got `?`; optional fields may only be defined \
								on flag fields"));
							}
						}
						_ => {
							return Err(parser_err!(next.span, "expected a `:` after the field name for its type, got `{next}`"));
						}
					}
					let refr = Parser::parse_reference(&mut peekable, &next.span, layer)?;
					let mut field_flags = None;
					match peekable.peek() {
						None => {},
						Some(dot) => {
							if dot.data == TokenData::Dot {
								peekable.next();
								let Some(curly) = peekable.next() else {
									return Err(parser_err!(token.span, "expected `{{}}` after `{}.`, found nothing - remove the period? (`.`)", refr.get_name()));
								};
								let inner = match &curly.data {
									TokenData::CurlyBraces(x) => x,
									_ => {
										return Err(parser_err!(curly.span, "expected `{{}}` after `{}.` - remove the period? (`.`)", refr.get_name()));
									}
								};
								let flags = Parser::parse_flags(&inner, layer)?;
								field_flags = Some(flags);
							}
						}
					}
					fields.push(Field {
						name: field_name.to_string(),
						name_span: token.span.clone(),
						value: refr,
						flags: field_flags,
						attrs: next_attrs,
						doc: next_doc.unwrap_or("").to_string()
					});
					next_doc = None;
					next_attrs = HashMap::new();
				}
				_ => {
					return Err(parser_err!(token.span, "unexpected token `{token}`; a field name should be followed by `:` and its type"));
				}
			}
		}

		Ok(FlexibleDeclarationValue::StructDeclaration { inline: false, fields, layer })
	}

	fn parse_enum_decl(tokens: &Vec<Token>, start_at_one: bool, layer: u32) -> Result<FlexibleDeclarationValue, PunybufError> {
		let mut variants = vec![];
		let mut peekable = tokens.iter().peekable();

		let mut counter: u8 = if start_at_one { 1 } else { 0 };
		let mut next_doc: Option<&str> = None;
		let mut next_attrs = HashMap::new();
		while let Some(tk) = peekable.next() {
			match &tk.data {
				TokenData::Attribute(attr, val) => {
					next_attrs.insert(attr.clone(), val.clone());
				}
				TokenData::Docs(doc) => {
					if let Some(_) = next_doc {
						return Err(parser_err!(tk.span, "documentation description defined twice"));
					};
					next_doc = Some(doc);
				}
				TokenData::Symbol(name) => {
					let mut value = None;

					match peekable.peek() {
						Some(Token { data: TokenData::Colon, span }) => {
							peekable.next(); // Consume the colon
							value = Some(Parser::parse_reference(&mut peekable, span, layer)?);
						}
						_ => {}
					}

					variants.push(EnumVariant {
						name: name.to_string(), name_span: tk.span.clone(),
						discriminant: counter,
						value,
						attrs: next_attrs,
						doc: next_doc.unwrap_or("").to_string()
					});
					next_doc = None;
					next_attrs = HashMap::new();
					counter += 1;
					match peekable.next() {
						None | Some(Token { data: TokenData::Comma, span: _ }) => {},
						Some(Token { data: _, span }) => {
							return Err(parser_err!(span, "expected a comma (`,`) to separate enum variants"));
						}
					}
				}
				_ => {
					return Err(parser_err!(tk.span, "unexpected token `{tk}`, enum variants must be separated by `,`"));
				}
			}
		}

		Ok(FlexibleDeclarationValue::EnumDeclaration { inline: false, variants, layer })
	}

	fn parse_value_enum_decl(tokens: &Vec<Token>, start_at_one: bool, layer: u32) -> Result<FlexibleDeclarationValue, PunybufError> {
		let mut variants = vec![];
		let mut peekable = tokens.iter().peekable();

		let mut counter: u8 = if start_at_one { 1 } else { 0 };
		let mut next_doc: Option<&str> = None;
		let mut next_attrs = HashMap::new();
		while let Some(tk) = peekable.peek() {
			match &tk.data {
				TokenData::Attribute(attr, val) => {
					next_attrs.insert(attr.clone(), val.clone());
				}
				TokenData::Docs(doc) => {
					if let Some(_) = next_doc {
						return Err(parser_err!(tk.span, "documentation description defined twice"));
					};
					next_doc = Some(doc);
				}
				TokenData::Symbol(_) => {
					let refr = Parser::parse_reference(&mut peekable, &Span::impossible(), layer)?;
					variants.push(ValueEnumVariant {
						discriminant: counter,
						value: refr,
						attrs: next_attrs,
						doc: next_doc.unwrap_or("").to_string()
					});
					next_doc = None;
					next_attrs = HashMap::new();
					counter += 1;
					match peekable.next() {
						None | Some(Token { data: TokenData::Comma, span: _ }) => {},
						Some(Token { data: _, span }) => {
							return Err(parser_err!(span, "expected a comma (`,`) to separate value-enum variants"));
						}
					}
				}
				_ => {
					return Err(parser_err!(tk.span, "unexpected token `{tk}`, value-enum variants must be separated by `,`"));
				}
			}
		}

		Ok(FlexibleDeclarationValue::ValueEnumDeclaration { inline: false, variants, layer })
	}

	fn parse_flags(tokens: &Vec<Token>, layer: u32) -> Result<Vec<FieldFlag>, PunybufError> {
		let mut peekable = tokens.iter().peekable();
		let mut flags = Vec::new();

		let mut next_doc: Option<&str> = None;
		let mut next_attrs = HashMap::new();

		while let Some(token) = peekable.next() {
			match &token.data {
				TokenData::Attribute(attr, val) => {
					next_attrs.insert(attr.clone(), val.clone());
				}
				TokenData::Docs(doc) => {
					if let Some(_) = next_doc {
						return Err(parser_err!(token.span, "documentaion description defined twice"));
					}
					next_doc = Some(doc);
				}
				TokenData::Symbol(flag_name) => {
					let question = peekable.next().ok_or(parser_err!(token.span, "expected a `?`, found nothing"))?;
					if question.data != TokenData::Question {
						return Err(parser_err!(token.span, "expected a `?` after the optional field's name"));
					}

					let mut refr = None;
					match peekable.peek() {
						Some(Token { data: TokenData::Colon, span }) => {
							peekable.next(); // Consumes the colon

							refr = Some(Parser::parse_reference(&mut peekable, span, layer)?);
							match peekable.peek() {
								Some(Token { data: TokenData::Dot, span: dot_span }) => {
									return Err(PunybufError {
										span: token.span.clone(),
										error: "flags (optional fields) cannot contain flag fields".to_string(),
										explanation: Some(
											ExtendedErrorExplanation::error_and(vec![
												InfoExplanation {
													content: format!("try removing this period, to make `{flag_name}` into a regular field"),
													span: dot_span.clone(),
													level: InfoLevel::Tip,
												},
												InfoExplanation {
													content: format!("...or try defining `{flag_name}`'s type so that it contains a flag field"),
													// if this is reached, refr is always `Some(...)`
													span: refr.unwrap().get_name_span().clone(),
													level: InfoLevel::Tip,
												},
											])
										)
									});
								}
								_ => {}
							}
						}
						_ => {}
					}
					flags.push(FieldFlag {
						name: flag_name.to_string(),
						name_span: token.span.clone(),
						value: refr,
						attrs: next_attrs,
						doc: next_doc.unwrap_or("").to_string()
					});
					next_doc = None;
					next_attrs = HashMap::new();
				}
				TokenData::Question => {
					return Err(parser_err!(token.span, "misplaced `?` (expected an identifier) - \
					have you forgotten to define a type for the previous flag?"));
				}
				_ => {
					return Err(parser_err!(token.span, "expected an identifier for a optional field name, got `{token}`; \
					a optional field identifier should be have a `?` at the end"));
				}
			}
		}

		Ok(flags)
	}

	/// Consumes the next token, which is expected to be a Symbol
	fn parse_reference(peekable: &mut Peekable<Iter<Token>>, before_sym: &Span, layer: u32) -> Result<ValueReference, PunybufError> {
		let thing = peekable.next().ok_or(parser_err!(before_sym, "expected an identifier, got nothing"))?;
		let name = match &thing.data {
			TokenData::Symbol(x) => x,
			_ => {
				return Err(parser_err!(thing.span, "expected an identifier, got `{thing}`"));
			}
		};

		match &peekable.peek() {
			Some(Token { data, span }) => {
				match data {
					TokenData::AngleBrackets(inside) => {
						peekable.next();
						let generics = Parser::parse_generics(&inside, layer)?;
						match peekable.peek() {
							Some(Token { data: TokenData::CurlyBraces(_), span: braces_span }) => {
								return Err(PunybufError {
									span: braces_span.clone(),
									error: format!("unexpected `{{ ... }}`; you cannot define generic arguments for inline declarations, such as `{name}`"),
									explanation: Some(
										ExtendedErrorExplanation::error_and(vec![
											InfoExplanation {
												content: format!("generics for `{name}` defined here"),
												span: span.clone(),
												level: InfoLevel::Info,
											}
										])
									)
								});
							}
							_ => {}
						};
						return Ok(ValueReference::Reference {
							name: name.to_string(),
							name_span: thing.span.clone(),
							generics, generic_span: span.clone(),
						});
					}
					TokenData::CurlyBraces(_) | TokenData::Parentheses(_) | TokenData::SquareBrackets(_) => {
						let (decl, decl_span) = Parser::parse_decl(
							peekable,
							&thing.span,
							true, false, layer
						)?;
						return Ok(ValueReference::InlineDeclaration {
							symbol: name.to_string(),
							name_span: thing.span.clone(),
							decl, decl_span
						});
					}
					_ => {}
				}
			}
			_ => {}
		};

		Ok(ValueReference::Reference {
			name: name.to_string(),
			name_span: thing.span.clone(),
			generics: vec![],
			generic_span: Span::impossible()
		})
	}
}