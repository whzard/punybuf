use std::{
	cmp, fmt::{Debug, Display}, iter::Peekable, rc::Rc,
};

use crate::errors::{ErrorInfo, PunybufError, parser_err, pb_err};

const COMMON: &str = include_str!("../baked/common.pbd");

#[derive(Debug, PartialEq, Eq)]
pub(crate) enum TokenData {
	Symbol(String),
	Numeric(u32),
	Equals,
	Colon,
	Semicolon,
	Comma,
	Dot,
	Arrow,
	Bang,
	Question,

	LayerKeyword,

	CurlyBraces(Vec<Token>),
	SquareBrackets(Vec<Token>),
	Parentheses(Vec<Token>),
	AngleBrackets(Vec<Token>),

	Docs(String),
	Attribute(String, Option<String>),
}

#[derive(Clone, PartialEq, Eq, PartialOrd, Ord, Hash)]
/// Position in a file
pub struct Loc {
	/// The 0-indexed row
	pub row: usize,
	/// The 0-indexed column
	pub col: usize,
}

impl Debug for Loc {
	fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
		write!(f, "{}:{}", self.row + 1, self.col + 1)
	}
}

impl Loc {
	/// Produces a Loc with both `row` and `col` set to `0`.
	pub fn zero() -> Self {
		Self { row: 0, col: 0 }
	}
}

/// A range in the source code file
#[derive(Clone, PartialEq, Eq, Hash)]
pub struct Span {
	pub(crate) loc_start: Loc,
	pub(crate) loc_end: Loc,
	pub(crate) file_name: Rc<String>,
	pub(crate) file_contents: Rc<String>,
}

impl Debug for Span {
	fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
		write!(f, "({}:{:?}-{:?})", self.file_name, self.loc_start, self.loc_end.col + 1)
	}
}

#[allow(dead_code)]
impl Span {
	/// Produces an empty invalid span, which cannot be
	/// displayed and does not have valid file contents.
	pub fn impossible() -> Self {
		Self {
			loc_start: Loc::zero(),
			loc_end: Loc::zero(),
			file_name: Rc::new("".to_string()),
			file_contents: Rc::new("".to_string())
		}
	}
	/// Produces a new span which spans from `self` until the `rhs`.
	/// If `self` is further than `rhs`, produces a clone of `self`.
	pub fn extend(&self, rhs: &Self) -> Self {
		let rightmost = cmp::max(&self.loc_end, &rhs.loc_end);
		Self {
			loc_start: self.loc_start.clone(),
			loc_end: rightmost.clone(),
			file_name: self.file_name.clone(),
			file_contents: self.file_contents.clone()
		}
	}
	/// Returns the file name. Note that the file name may be empty
	/// in case this Span is [`Span::impossible`]. That can happen,
	/// for example, when converting from JSON IR to the Punybuf definition.
	pub fn file_name(&self) -> &str {
		&self.file_name
	}
	/// Returns the start location of the span.
	/// Note that it might be the same as the end location.
	pub fn start(&self) -> &Loc {
		&self.loc_start
	}
	/// Returns the end location of the span.
	/// Note that it might be the same as the start location.
	pub fn end(&self) -> &Loc {
		&self.loc_end
	}
}

/// The output of [`Lexer`].
///
/// This is intentionally opaque as modifying tokens
/// directly is not stable and currently unsupported.
pub struct LexerOutput {
	pub(crate) tokens: Vec<Token>
}

impl LexerOutput {
	/// Returns an empty output, as if the lexer did not produce anything.
	pub fn empty() -> Self {
		Self { tokens: vec![] }
	}
	fn into_tokens(self) -> Vec<Token> {
		self.tokens
	}
	fn from_tokens(tokens: Vec<Token>) -> Self {
		Self { tokens }
	}
}

#[derive(PartialEq, Eq)]
pub(crate) struct Token {
	pub(crate) data: TokenData,
	pub(crate) span: Span
}

impl Token {
	fn new(data: TokenData, loc: Loc, file_name: Rc<String>, file_contents: Rc<String>) -> Self {
		let mut loc_end = Loc { row: loc.row, col: loc.col + 1 };
		match &data {
			TokenData::AngleBrackets(inner) |
			TokenData::SquareBrackets(inner) |
			TokenData::CurlyBraces(inner) |
			TokenData::Parentheses(inner) => {
				if let Some(last) = inner.last() {
					// If the block is empty, this doesn't really work
					// but whatever lol
					loc_end = last.span.loc_end.clone();
				}
			}
			TokenData::Symbol(string) => {
				loc_end.col = loc.col + string.len();
			}
			TokenData::Attribute(name, value) => {
				loc_end.col = loc.col + name.len();

				if let Some(value) = value {
					let mut current_row = loc_end.row;
					let mut current_col = loc_end.col + "(".len();

					// Not the fastest way, since we already iterate over
					// all the chars in docs, but works for now
					for ch in value.chars() {
						if ch == '\n' {
							current_col = 0;
							current_row += 1;
						} else {
							current_col += 1;
						}
					}
					current_col += ")".len();
					loc_end.row = current_row;
					loc_end.col = current_col;
				}
			}
			TokenData::Numeric(n) => {
				loc_end.col = loc.col + n.to_string().len();
			}
			TokenData::Arrow => {
				loc_end.col = loc.col + 2;
			}
			TokenData::Docs(docs) => {
				let mut current_row = loc.row;
				let mut current_col = loc.col + "#[".len();

				// Not the fastest way, since we already iterate over
				// all the chars in docs, but works for now
				for ch in docs.chars() {
					if ch == '\n' {
						current_col = 0;
						current_row += 1;
					} else {
						current_col += 1;
					}
				}
				current_col += "]".len();
				loc_end = Loc { col: current_col, row: current_row }
			}
			TokenData::LayerKeyword => {
				loc_end.col = loc.col + "layer".len();
			},
			TokenData::Equals | TokenData::Colon | TokenData::Comma |
			TokenData::Semicolon | TokenData::Bang | TokenData::Dot |
			TokenData::Question => {}
		};
		Self {
			data, span: Span { loc_start: loc, loc_end, file_name, file_contents }
		}
	}
}

impl Debug for Token {
	fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
		write!(f, "<{:?} at {:?}>", self.data, self.span)?;
		Ok(())
	}
}

impl Display for Token {
	fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
		write!(f, "{}", match &self.data {
			TokenData::AngleBrackets(_) => "< ... >".to_string(),
			TokenData::SquareBrackets(_) => "[ ... ]".to_string(),
			TokenData::CurlyBraces(_) => "{ ... }".to_string(),
			TokenData::Parentheses(_) => "( ... )".to_string(),
			TokenData::Docs(_) => "#[ ... ]".to_string(),
			TokenData::LayerKeyword => "layer".to_string(),
			TokenData::Bang => "!".to_string(),
			TokenData::Question => "?".to_string(),
			TokenData::Dot => ".".to_string(),
			TokenData::Colon => ":".to_string(),
			TokenData::Comma => ",".to_string(),
			TokenData::Semicolon => ";".to_string(),
			TokenData::Equals => "=".to_string(),
			TokenData::Arrow => "->".to_string(),
			TokenData::Numeric(n) => n.to_string(),
			TokenData::Symbol(val) => val.clone(),
			TokenData::Attribute(attr, val) =>
				if let Some(val) = val { format!("{}({})", attr, val) } else { attr.clone() },
		})?;
		Ok(())
	}
}

/// A trait to handle the `include` keyword.
pub trait IncludeHandler {
	/// Handle the `include` keyword. The `include_path` is the trimmed string passed
	/// after the `include` keyword. Including `common` is handled automatically
	/// and this function will not get called in that case.
	fn handle_include(&mut self, include_path: String, include_span: Span)
		-> Result<LexerOutput, PunybufError>;
}

/// Set this as the [`IncludeHandler`] to disallow including other files.
pub struct IncludeDisallowed;
impl IncludeHandler for IncludeDisallowed {
	fn handle_include(&mut self, _: String, include_span: Span) -> Result<LexerOutput, PunybufError> {
		Err(pb_err!(include_span, "include is not allowed here".to_string(), ErrorInfo::empty()))
	}
}

/// The [lexer](https://en.wikipedia.org/wiki/Lexical_analysis)
/// for Punybuf definition files. Use this to produce a `LexerOutput`,
/// which would be the output of the include handler.
/// 
/// Note that even though parsing is as simple as just feeding all the
/// tokens to the internal Punybuf parser, this API is intentionally hidden
/// because it is unstable. For a way to quickly lex and parse a definiton file,
/// use a [`crate::PunybufParser`].
//        ^ This will give out a warning when compiling for
// an executable
pub struct Lexer<'a, I> {
	pub(crate) contents: Rc<String>,
	pub(crate) file_name: Rc<String>,
	pub(crate) current_loc: Loc,
	pub(crate) include_handler: &'a mut I,
	pub(crate) includes_common: bool,
}

impl<'a, I: IncludeHandler> Lexer<'a, I> {
	/// Construct a new Lexer with the specified `include_handler`
	pub fn new(contents: String, file_name: String, include_handler: &'a mut I) -> Self {
		let contents = Rc::new(contents);
		let file_name = Rc::new(file_name);

		Self {
			file_name,
			contents,
			current_loc: Loc::zero(),
			include_handler,
			includes_common: false,
		}
	}
	fn implicit_layer_definition(&self) -> Span {
		const IMPLICIT_LAYER_DEFINITION: &str = "<implicit layer definition>";
		Span {
			loc_start: Loc { row: self.current_loc.row + 1, col: 0 },
			loc_end: Loc { row: self.current_loc.row + 1, col: IMPLICIT_LAYER_DEFINITION.len() },
			file_name: self.file_name.clone(),
			file_contents: Rc::new("\n".repeat(self.current_loc.row + 1) + IMPLICIT_LAYER_DEFINITION)
		}
	}
	/// Produce the lexer output
	pub fn lex(&mut self) -> Result<LexerOutput, PunybufError> {
		self.includes_common = false;

		let mut tokens: Vec<Token> = Vec::new();
		let x = self.contents.clone();
		let mut peekable = x.chars().peekable();

		self.lex_internal(&mut tokens, &mut peekable, None)?;

		// Implicit `layer 0:` in case this file is included
		// (this prevents accidental "time-travel" when a type
		// from a lower layer references a type from a higher layer)
		tokens.push(Token {
			data: TokenData::LayerKeyword,
			span: self.implicit_layer_definition()
		});
		tokens.push(Token {
			data: TokenData::Numeric(0),
			span: self.implicit_layer_definition()
		});
		tokens.push(Token {
			data: TokenData::Colon,
			span: self.implicit_layer_definition()
		});

		return Ok(LexerOutput::from_tokens(tokens));
	}
	pub(crate) fn token(&self, data: TokenData) -> Token {
		Token::new(data, self.current_loc.clone(), self.file_name.clone(), self.contents.clone())
	}
	pub(crate) fn token_end_loc(&self, data: TokenData, loc_end: Loc) -> Token {
		Token {
			data, span: Span {
				loc_start: self.current_loc.clone(),
				loc_end, file_name: self.file_name.clone(),
				file_contents: self.contents.clone()
			}
		}
	}
	fn include(&mut self, include_path: String, include_span: Span)
		-> Result<LexerOutput, PunybufError>
	{
		if include_path == "common" {
			if self.includes_common {
				// Including common multiple times is okay
				return Ok(LexerOutput::empty());
			}
			self.includes_common = true;
			let mut rust_is_funny = IncludeDisallowed;
			let mut l = Lexer::new(COMMON.to_string(), "<common>".into(), &mut rust_is_funny);
			return l.lex();
		}
		self.include_handler.handle_include(include_path, include_span)
	}
	fn lex_error(&self, error: String) -> PunybufError {
		pb_err!(
			Span {
				loc_start: self.current_loc.clone(),
				loc_end: Loc { row: self.current_loc.row, col: self.current_loc.col + 1 },
				file_name: self.file_name.clone(),
				file_contents: self.contents.clone()
			},
			error
		)
	}
	fn lex_internal<Iter>(
		&mut self, tokens: &mut Vec<Token>, peekable: &mut Peekable<Iter>, stop_on: Option<char>
	) -> Result<bool, PunybufError>
		where Iter: Iterator<Item = char>
	{
		while let Some(ch) = peekable.next() {
			match ch {
				'#' => {
					if let Some(chn) = peekable.peek() {
						if *chn == '[' {
							peekable.next();

							let mut doc = String::new();

							let mut nesting = 1;
							let mut stopped = false;
							while let Some(x) = peekable.next() {
								if x == ']' {
									nesting -= 1;
									if nesting <= 0 {
										stopped = true;
										break;
									}
								}
								if x == '[' {
									nesting += 1;
								}

								doc.push(x);
							}

							if !stopped {
								return Err(self.lex_error(format!(
									"expected a closing bracket (`]`) to end the doc-comment at {}:{}:{}",
									self.file_name,
									self.current_loc.row + 1, self.current_loc.col + 1
								)));
							}

							let doc_token = self.token(TokenData::Docs(doc));
							self.current_loc = doc_token.span.loc_end.clone();
							tokens.push(doc_token);

						} else {
							while let Some(x) = peekable.next() {
								self.current_loc.col += 1;
								if x == '\n' {
									self.current_loc.col = 0;
									self.current_loc.row += 1;
									break;
								}
							}
						}
						continue;
					}
				}
				' ' | '\r' | '\t' => {}
				'\n' => {
					self.current_loc.col = 0;
					self.current_loc.row += 1;
					continue; // Skip advancing the column
				}
				'=' => tokens.push(self.token(TokenData::Equals)),
				':' => tokens.push(self.token(TokenData::Colon)),
				'!' => tokens.push(self.token(TokenData::Bang)),
				'?' => tokens.push(self.token(TokenData::Question)),
				';' => tokens.push(self.token(TokenData::Semicolon)),
				',' => tokens.push(self.token(TokenData::Comma)),
				'.' => tokens.push(self.token(TokenData::Dot)),
				'-' => {
					if let Some(chn) = peekable.next() {
						if chn != '>' {
							return Err(self.lex_error(format!("expected `>` to make an arrow (`->`), found `{chn}`")));
						}
						tokens.push(self.token(TokenData::Arrow));
						self.current_loc.col += 1;
					} else {
						return Err(self.lex_error(format!("expected `>` to make an arrow (`->`), found nothing")));
					}
				},
				'{' => {
					let mut inside: Vec<Token> = Vec::new();
					let loc_begin = self.current_loc.clone();
					self.current_loc.col += 1;
					let stopped = self.lex_internal(&mut inside, peekable, Some('}'))?;
					let loc_end = self.current_loc.clone();
					if !stopped {
						return Err(self.lex_error(format!(
							"expected a closing brace (`}}`) to match one at {}:{}:{}",
							self.file_name,
							loc_begin.row + 1, loc_begin.col + 1
						)));
					}

					self.current_loc = loc_begin;
					let bracks = self.token_end_loc(TokenData::CurlyBraces(inside), loc_end.clone());
					self.current_loc = loc_end;
					tokens.push(bracks);
					continue;
				}
				'[' => {
					let mut inside: Vec<Token> = Vec::new();
					let loc_begin = self.current_loc.clone();
					self.current_loc.col += 1;
					let stopped = self.lex_internal(&mut inside, peekable, Some(']'))?;
					let loc_end = self.current_loc.clone();
					if !stopped {
						return Err(self.lex_error(format!(
							"expected a closing bracket (`]`) to match one at {}:{}:{}",
							self.file_name,
							loc_begin.row + 1, loc_begin.col + 1
						)));
					}

					self.current_loc = loc_begin;
					let bracks = self.token_end_loc(TokenData::SquareBrackets(inside), loc_end.clone());
					self.current_loc = loc_end;
					tokens.push(bracks);
					continue;
				}
				'(' => {
					let mut inside: Vec<Token> = Vec::new();
					let loc_begin = self.current_loc.clone();
					self.current_loc.col += 1;
					let stopped = self.lex_internal(&mut inside, peekable, Some(')'))?;
					let loc_end = self.current_loc.clone();
					if !stopped {
						return Err(self.lex_error(format!(
							"expected a closing parenthesis (`)`) to match one at {}:{}:{}",
							self.file_name,
							loc_begin.row + 1, loc_begin.col + 1
						)));
					}

					self.current_loc = loc_begin;
					let bracks = self.token_end_loc(TokenData::Parentheses(inside), loc_end.clone());
					self.current_loc = loc_end;
					tokens.push(bracks);
					continue;
				}
				'<' => {
					let mut inside: Vec<Token> = Vec::new();
					let loc_begin = self.current_loc.clone();
					self.current_loc.col += 1;
					let stopped = self.lex_internal(&mut inside, peekable, Some('>'))?;
					let loc_end = self.current_loc.clone();
					if !stopped {
						return Err(self.lex_error(format!(
							"expected a closing angle bracket (`>`) to match one at {}:{}:{}",
							self.file_name,
							loc_begin.row + 1, loc_begin.col + 1
						)));
					}

					self.current_loc = loc_begin;
					let bracks = self.token_end_loc(TokenData::AngleBrackets(inside), loc_end.clone());
					self.current_loc = loc_end;
					tokens.push(bracks);
					continue;
				}
				'@' => {
					let mut attr = ch.to_string();
					let mut value: Option<String> = None;
					while let Some(chn) = peekable.peek() {
						if chn.is_whitespace() {
							break;
						} else if *chn == '(' {
							_ = peekable.next().unwrap();
							let mut string = String::new();

							let mut nest_level = 0;
							let mut stopped = false;
							while let Some(chn) = peekable.next() {
								if chn == ')' {
									if nest_level <= 0 {
										stopped = true;
										break;
									} else {
										nest_level -= 1;
									}
								}
								if chn == '(' {
									nest_level += 1;
								}
								string.push(chn);
							}

							if !stopped {
								return Err(self.lex_error(format!(
									"expected a closing parenthesis (`)`) to end the attribute at {}:{}:{}",
									self.file_name,
									self.current_loc.row + 1, self.current_loc.col + 1
								)));
							}

							value = Some(string);

							break;
						} else {
							let chn = peekable.next().unwrap();
							attr.push(chn);
						}
					}
					let tk = self.token(TokenData::Attribute(attr, value));
					self.current_loc = tk.span.loc_end.clone();
					tokens.push(tk);
					continue;
				}
				_ => {
					if Some(ch) == stop_on {
						self.current_loc.col += 1;
						return Ok(true);
					}
					if ch.is_alphabetic() || ch == '_' {
						let mut symbol = ch.to_string();
						while let Some(chn) = peekable.peek() {
							if chn.is_alphanumeric() || *chn == '_' {
								let chn = peekable.next().unwrap();
								symbol.push(chn);
							} else {
								break;
							}
						}

						match symbol.as_str() {
							"include" => {
								let mut path = String::new();
								let mut whitespace_len = 0;
								while let Some(chn) = peekable.peek() {
									if *chn == '\n' {
										break;
									}

									let chn = peekable.next().unwrap();
									if path.is_empty() {
										match chn {
											' ' | '\t' => {
												whitespace_len += 1;
												continue;
											},
											_ => {}
										}
									}
									path.push(chn);
								}
								self.current_loc.col += "include".len() + whitespace_len;
								let loc_start = self.current_loc.clone();
								let loc_end = Loc {
									row: loc_start.row,
									col: loc_start.col + path.len(),
								};

								self.current_loc = loc_end.clone();
								let included_tokens = self.include(path, Span {
									loc_start, loc_end, file_name: self.file_name.clone(),
									file_contents: self.contents.clone()
								})?;

								tokens.append(&mut included_tokens.into_tokens());
							}
							"layer" => {
								tokens.push(self.token(TokenData::LayerKeyword));
								self.current_loc.col += "layer".len();
							}
							_ => {
								let tk = self.token(TokenData::Symbol(symbol));

								self.current_loc = tk.span.loc_end.clone();
								tokens.push(tk);
							}
						}

						continue;

					} else if ch.is_ascii_digit() {
						let mut string = ch.to_string();
						while let Some(chn) = peekable.peek() {
							if chn.is_ascii_digit() || *chn == '_' {
								let chn = peekable.next().unwrap();
								string.push(chn);
							} else {
								break;
							}
						}

						let number: u32 = match string.parse() {
							Ok(x) => x,
							Err(err) => {
								let mut loc_end = self.current_loc.clone();
								loc_end.col += string.len();

								return Err(parser_err!(Span {
									loc_start: self.current_loc.clone(), loc_end,
									file_name: self.file_name.clone(),
									file_contents: self.contents.clone()
								}, "invalid number: {err}"))
							}
						};
						tokens.push(self.token(TokenData::Numeric(number)));

					} else {
						return Err(self.lex_error(format!("unexpected character '{ch}', wild!")));
					}
				}
			}
			// Some branches don't need this, so they use `continue`
			self.current_loc.col += 1;
		}
		return Ok(false);
	}
}