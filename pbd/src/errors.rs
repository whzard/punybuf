// TODO: rewrite the entire error interface, because it sucks to use rn
// 😭

use std::fmt::Display;

use crate::lexer::Span;

#[derive(Debug)]
pub struct PunybufError {
	pub span: Span,
	pub error: String,
	pub info: ErrorInfo
}

impl Display for PunybufError {
	fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
		write!(f, "{}\n{}", self.error, self.info.explain(&self))
	}
}

pub const RED: &str = "\x1b[91m";
pub const BLUE: &str = "\x1b[94m";
pub const YELLOW: &str = "\x1b[93m";
pub const NORMAL: &str = "\x1b[0m";
pub const GRAY: &str = "\x1b[30m";
pub const GREEN: &str = "\x1b[32m";
#[allow(unused)]
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
pub struct Diagnostic {
	pub content: String,
	pub span: Span,
	pub level: InfoLevel,
}
impl Diagnostic {
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
pub struct ErrorInfo {
	pub before_error: Vec<Diagnostic>,
	pub after_error: Vec<Diagnostic>,
	pub explain_error: bool
}
#[allow(unused)]
impl ErrorInfo {
	pub fn empty() -> Self {
		Self {
			before_error: Vec::new(),
			after_error: Vec::new(),
			explain_error: true
		}
	}
	pub fn and_error(before_error: Vec<Diagnostic>) -> Self {
		Self {
			before_error,
			after_error: Vec::new(),
			explain_error: true
		}
	}
	pub fn error_and(after_error: Vec<Diagnostic>) -> Self {
		Self {
			before_error: Vec::new(),
			after_error,
			explain_error: true
		}
	}
	pub fn custom(vec: Vec<Diagnostic>) -> Self {
		Self {
			before_error: vec,
			after_error: Vec::new(),
			explain_error: false
		}
	}
}

impl ErrorInfo {
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
			result.push_str(&Diagnostic::from_error(error).explain());
		}

		for info in &self.after_error {
			result.push_str("\n\n");
			result.push_str(&info.explain());
		}

		result
	}
}

#[macro_export]
/// (span: Span, error: String, info: ErrorInfo)
macro_rules! pb_err {
	($span:expr, $err:expr, $expl:expr) => {
		PunybufError {
			span: $span.clone(),
			error: $err,
			info: $expl,
		}
	};
	($span:expr, $err:expr) => {
		PunybufError {
			span: $span.clone(),
			error: $err,
			info: ErrorInfo::empty(),
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
			info: ErrorInfo::empty(),
		}
	};
	($span:expr, $string:literal) => {
		PunybufError {
			span: $span.clone(),
			error: format!($string),
			info: ErrorInfo::empty(),
		}
	};
}

pub(crate) use parser_err;