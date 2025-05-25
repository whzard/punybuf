use std::{fmt::{Debug, Display}, iter::Peekable, rc::Rc, str::Chars};

use crate::errors::{parser_err, ExtendedErrorExplanation, PunybufError};

#[derive(Debug, PartialEq, Eq)]
pub enum TokenData {
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

#[derive(Clone, PartialEq, Eq)]
pub struct Loc {
	pub row: usize,
	pub col: usize,
}

impl Debug for Loc {
	fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
		write!(f, "{}:{}", self.row + 1, self.col + 1)
	}
}

impl Loc {
	pub fn zero() -> Self {
		Self { row: 0, col: 0 }
	}
}

#[derive(Clone, PartialEq, Eq)]
pub struct Span {
	pub loc_start: Loc,
	pub loc_end: Loc,
	pub file_name: String,
	pub file_contents: Rc<String>,
}

impl Debug for Span {
	fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
		write!(f, "({}:{:?}-{:?})", self.file_name, self.loc_start, self.loc_end.col + 1)
	}
}

impl Span {
	pub fn impossible() -> Self {
		Self {
			loc_start: Loc::zero(),
			loc_end: Loc::zero(),
			file_name: "".to_string(),
			file_contents: Rc::new("".to_string())
		}
	}
	pub fn full_line(all: &str, file_name: String) -> Self {
		Self {
			loc_start: Loc::zero(),
			loc_end: Loc { col: all.len(), row: 0 },
			file_name,
			file_contents: Rc::new(all.to_string())
		}
	}
	/// Produces a new span which spans from self until the `rhs`
	pub fn extend(&self, rhs: &Self) -> Self {
		Self {
			loc_start: self.loc_start.clone(),
			loc_end: rhs.loc_end.clone(),
			file_name: self.file_name.clone(),
			file_contents: self.file_contents.clone()
		}
	}
}

#[derive(PartialEq, Eq)]
pub struct Token {
	pub data: TokenData,
	pub span: Span
}

impl Token {
	fn new(data: TokenData, loc: Loc, file_name: String, file_contents: Rc<String>) -> Self {
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
				loc_end.col = loc.col + name.len() + if let Some(value) = value {
					value.len() + 2 // length of "()"
				} else {
					0
				};
			}
			TokenData::Numeric(n) => {
				loc_end.col = loc.col + n.to_string().len();
			}
			TokenData::Arrow => {
				loc_end.col = loc.col + 2;
			}
			TokenData::Docs(docs) => {
				let mut current_row = loc.row;
				let mut current_col = loc.col + 3;

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

pub trait IncludeHandler {
	fn handle_include(&mut self, include_path: String, include_span: Span) -> Result<Vec<Token>, PunybufError>;
}

pub struct Lexer<'a, I> {
	pub contents: Rc<String>,
	pub file_name: &'a str,
	pub current_loc: Loc,
	pub include_handler: &'a mut I,
	pub includes_common: bool,
}

impl<'a, I: IncludeHandler> Lexer<'a, I> {
	pub fn new(contents: String, file_name: &'a str, include_handler: &'a mut I) -> Self {
		let rc = Rc::new(contents);

		Self {
			file_name,
			contents: rc,
			current_loc: Loc::zero(),
			include_handler,
			includes_common: false,
		}
	}
	pub fn lex(&mut self) -> Result<Vec<Token>, PunybufError> {
		self.includes_common = false;

		let mut tokens: Vec<Token> = Vec::new();
		let x = self.contents.clone();
		let mut peekable = x.chars().peekable();

		self.lex_internal(&mut tokens, &mut peekable, None)?;

		return Ok(tokens);
	}
	pub fn token(&self, data: TokenData) -> Token {
		Token::new(data, self.current_loc.clone(), self.file_name.to_string(), self.contents.clone())
	}
	pub fn token_end_loc(&self, data: TokenData, loc_end: Loc) -> Token {
		Token {
			data, span: Span {
				loc_start: self.current_loc.clone(),
				loc_end, file_name: self.file_name.to_string(),
				file_contents: self.contents.clone()
			}
		}
	}
	fn lex_error(&self, error: String) -> PunybufError {
		PunybufError {
			span: Span {
				loc_start: self.current_loc.clone(),
				loc_end: Loc { row: self.current_loc.row, col: self.current_loc.col + 1 },
				file_name: self.file_name.to_string(),
				file_contents: self.contents.clone()
			},
			error,
			explanation: Some(ExtendedErrorExplanation::empty())
		}
	}
	fn lex_internal(&mut self, tokens: &mut Vec<Token>, peekable: &mut Peekable<Chars<'_>>, stop_on: Option<char>) -> Result<bool, PunybufError> {
		while let Some(ch) = peekable.next() {
			match ch {
				'#' => {
					if let Some(chn) = peekable.peek() {
						if *chn == '[' {
							peekable.next();

							let mut doc = String::new();

							let mut nesting = 1;
							while let Some(x) = peekable.next() {
								if x == ']' {
									nesting -= 1;
									if nesting <= 0 {
										break;
									}
								}
								if x == '[' {
									nesting += 1;
								}

								doc.push(x);
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
							return Err(self.lex_error(format!("Expected `>` to make an arrow (`->`), found `{chn}`")));
						}
						tokens.push(self.token(TokenData::Arrow));
						self.current_loc.col += 1;
					} else {
						return Err(self.lex_error(format!("Expected `>` to make an arrow (`->`), found nothing")));
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
							"Expected a closing brace (`}}`) to match one at {}:{}:{}",
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
							"Expected a closing bracket (`]`) to match one at {}:{}:{}",
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
							"Expected a closing parenthesis (`)`) to match one at {}:{}:{}",
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
							"Expected a closing angle bracket (`>`) to match one at {}:{}:{}",
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
							while let Some(chn) = peekable.next() {
								if chn == ')' {
									if nest_level <= 0 {
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

								if path == "common" {
									self.includes_common = true;
								}

								self.current_loc = loc_end.clone();
								/* let mut included_tokens = (self.include_fn)(path, Span {
									loc_start, loc_end, file_name: self.file_name.to_string(),
									file_contents: self.contents.clone()
								})?; */
								let mut included_tokens = self.include_handler.handle_include(path, Span {
									loc_start, loc_end, file_name: self.file_name.to_string(),
									file_contents: self.contents.clone()
								})?;

								tokens.append(&mut included_tokens);
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
									file_name: self.file_name.to_string(),
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