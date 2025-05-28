// TODO: rewrite the entire error interface, because it sucks to use rn
// 😭

use std::fmt::Display;

use crate::lexer::Span;

pub trait ErrorExplanation {
	fn explain(&self, error: &PunybufError) -> String;
}

#[derive(Debug)]
pub struct PunybufError {
	pub span: Span,
	pub error: String,
	pub explanation: Option<ExtendedErrorExplanation>
}

impl Display for PunybufError {
	fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
		match &self.explanation {
			None => {
				write!(f, "{} at {}:{}:{}",
					self.error,
					self.span.file_name,
					self.span.loc_start.row + 1, self.span.loc_start.col + 1,
				)
			}
			Some(e) => {
				write!(f, "{}\n{}", self.error, e.explain(&self))
			}
		}
	}
}

macro_rules! error_tk {
	($token:ident, $string:literal, $($rpt:expr),+) => {
		Err(PunybufError {
			span: $token.span,
			error: format!($string, $($rpt),+),
			explanation: None
		})
	};
}


pub const RED: &str = "\x1b[91m";
pub const BLUE: &str = "\x1b[94m";
pub const YELLOW: &str = "\x1b[93m";
pub const NORMAL: &str = "\x1b[0m";
pub const GRAY: &str = "\x1b[30m";
pub const GREEN: &str = "\x1b[32m";
pub const INTENSE: &str = "\x1b[97m";
pub const BOLD: &str = "\x1b[1m";


#[derive(Debug)]
pub enum InfoLevel {
	Error, Warning, Tip, Info
}
impl InfoLevel {
	pub fn get_ansi_color(&self) -> &str {
		match self {
			Self::Error => RED,
			Self::Tip | Self::Info => BLUE,
			Self::Warning => YELLOW
		}
	}
	pub fn get_symbol(&self) -> &str {
		match self {
			Self::Error | Self::Warning => "~",
			Self::Tip | Self::Info => "-",
		}
	}
}

// TODO: rework this whole system because it's really
// crappy. Ideally make `PunybufError` just a case of
// `InfoExplanation`
#[derive(Debug)]
pub struct InfoExplanation {
	pub content: String,
	pub span: Span,
	pub level: InfoLevel,
}
impl InfoExplanation {
	pub fn from_error(error: &PunybufError) -> Self {
		Self {
			content: error.error.clone(),
			span: error.span.clone(),
			level: InfoLevel::Error
		}
	}
	pub fn explain(&self) -> String {
		let contents = self.span.file_contents.clone();
		let line = contents.lines()
			.nth(self.span.loc_start.row)
			.unwrap_or(&("?".to_string() + &".".repeat(self.span.loc_end.col.saturating_sub(1))))
			.replace("\t", " ");

		let extend_for = if self.span.loc_start.row == self.span.loc_end.row {
			self.span.loc_end.col - self.span.loc_start.col
		} else {
			line.len() - self.span.loc_start.col + 1
		};

		let color = self.level.get_ansi_color();
		let symbol = self.level.get_symbol();

		format!("\
{BLUE}--> {GRAY}{}:{}:{}
{BLUE}    |
{: >3} | {NORMAL}{}
{BLUE}    | {}{BOLD}{color}{}{NORMAL}{color} {}{NORMAL}\
",
			self.span.file_name, self.span.loc_start.row + 1, self.span.loc_start.col + 1,
			self.span.loc_start.row + 1,
			line,
			" ".repeat(self.span.loc_start.col),
			symbol.repeat(extend_for),
			self.content
		)
	}
}

#[derive(Debug)]
pub struct ExtendedErrorExplanation {
	pub before_error: Vec<InfoExplanation>,
	pub after_error: Vec<InfoExplanation>,
	pub explain_error: bool
}
impl ExtendedErrorExplanation {
	pub fn empty() -> Self {
		Self {
			before_error: Vec::new(),
			after_error: Vec::new(),
			explain_error: true
		}
	}
	pub fn and_error(before_error: Vec<InfoExplanation>) -> Self {
		Self {
			before_error,
			after_error: Vec::new(),
			explain_error: true
		}
	}
	pub fn error_and(after_error: Vec<InfoExplanation>) -> Self {
		Self {
			before_error: Vec::new(),
			after_error,
			explain_error: true
		}
	}
	pub fn custom(vec: Vec<InfoExplanation>) -> Self {
		Self {
			before_error: vec,
			after_error: Vec::new(),
			explain_error: false
		}
	}
}

impl ErrorExplanation for ExtendedErrorExplanation {
	fn explain(&self, error: &PunybufError) -> String {
		let mut result = String::new();
		for (i, info) in self.before_error.iter().enumerate() {
			if i != 0 {
				result.push_str("\n\n");
			}
			result.push_str(&info.explain());
		}
		
		if self.explain_error {
			if !self.before_error.is_empty() {
				result.push_str("\n\n");
			}
			result.push_str(&InfoExplanation::from_error(error).explain());
		}

		for info in &self.after_error {
			result.push_str("\n\n");
			result.push_str(&info.explain());
		}

		result
	}
}

#[macro_export]
/// (span: Span, error: String, explanation: ExtendedErrorExplanation)
macro_rules! pb_err {
	($span:expr, $err:expr, $expl:expr) => {
		PunybufError {
			span: $span.clone(),
			error: $err,
			explanation: Some($expl),
		}
	};
	($span:expr, $err:expr) => {
		PunybufError {
			span: $span.clone(),
			error: $err,
			explanation: Some(ExtendedErrorExplanation::empty()),
		}
	};
}

pub(crate) use pb_err;

#[macro_export]
macro_rules! parser_err {
	($span:expr, $string:literal, $($rpt:expr),+) => {
		PunybufError {
			span: $span.clone(),
			error: format!($string, $($rpt),+),
			explanation: Some(ExtendedErrorExplanation::empty()),
		}
	};
	($span:expr, $string:literal) => {
		PunybufError {
			span: $span.clone(),
			error: format!($string),
			explanation: Some(ExtendedErrorExplanation::empty()),
		}
	};
}

pub(crate) use parser_err;