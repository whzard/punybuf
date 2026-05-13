// TODO: rewrite the entire error interface, because it sucks to use rn
// 😭

use std::fmt::Display;

use crate::lexer::Span;

#[derive(Debug)]
pub struct PunybufError {
	pub display_error: bool,
	pub error: Diagnostic,
	pub before_error: Vec<Diagnostic>,
	pub after_error: Vec<Diagnostic>,
}

impl PunybufError {
	pub fn default() -> Self {
		Self {
			error: Diagnostic {
				content: "".into(), span: Span::impossible(), level: InfoLevel::Info
			},
			display_error: true, before_error: vec![], after_error: vec![]
		}
	}
	pub fn wrap_before(self, mut wrapper: PunybufError) -> PunybufError {
		for d in self.before_error {
			wrapper.before_error.push(d);
		}
		wrapper.before_error.push(self.error);
		for d in self.after_error {
			wrapper.before_error.push(d);
		}

		wrapper
	}
	pub fn wrap_after(self, mut wrapper: PunybufError) -> PunybufError {
		for d in self.before_error {
			wrapper.after_error.push(d);
		}
		wrapper.after_error.push(self.error);
		for d in self.after_error {
			wrapper.after_error.push(d);
		}

		wrapper
	}
	fn explain(&self) -> String {
		let mut result = String::new();
		for (i, info) in self.before_error.iter().enumerate() {
			if i != 0 {
				result.push_str("\n\n");
			}
			result.push_str(&info.explain());
		}
		
		if self.display_error {
			if !self.before_error.is_empty() {
				result.push_str("\n\n");
			}
			result.push_str(&self.error.explain());
		}

		for info in &self.after_error {
			result.push_str("\n\n");
			result.push_str(&info.explain());
		}

		result
	}
}

impl Display for PunybufError {
	fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
		write!(f, "{}\n{}", self.error.content, self.explain())
	}
}

pub const RED: &str = "\x1b[91m";
pub const BLUE: &str = "\x1b[94m";
pub const YELLOW: &str = "\x1b[93m";
pub const NORMAL: &str = "\x1b[0m";
pub const GRAY: &str = "\x1b[30m";
#[allow(unused)] // this is not true, this constant is used in main.rs
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

fn byte_index(string: &str, idx: usize) -> usize {
	string.char_indices().nth(idx).unwrap_or((string.len(), ' ')).0
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
	pub fn explain(&self) -> String {
		if self.span == Span::impossible() {
			let color = self.level.get_ansi_color();
			return format!(
				// help i have no idea how to make it
				// pretty
				"{color}    {BOLD}-{NORMAL}{color} {content}{NORMAL}",
				content = self.content
			)
		}
		let contents = self.span.file_contents.clone();

		let color = self.level.get_ansi_color();
		let symbol = self.level.get_symbol();

		let mut extend_for = (
			self.span.loc_end.col as isize - self.span.loc_start.col as isize
		).unsigned_abs();

		let mut lines = String::new();
		for (row, line) in contents.lines().enumerate().skip(self.span.loc_start.row) {
			if row > self.span.loc_end.row { break }
			let mut fmt_line = line.replace("\t", " ");
			if row == self.span.loc_start.row {
				fmt_line.insert_str(
					byte_index(&fmt_line, self.span.loc_start.col),
					color
				);
			} else {
				fmt_line.insert_str(0, color);
			}
			if row == self.span.loc_end.row {
				fmt_line.insert_str(
					byte_index(&fmt_line, self.span.loc_end.col + color.len()),
					NORMAL
				);
			}
			lines.push_str(&format!(
				"{BLUE}{row: >3} | {NORMAL}{line}\n",
				row = row + 1,
				line = fmt_line
			));
			let len = line.chars().count();
			if
				row != self.span.loc_end.row &&
				row != self.span.loc_start.row &&
				len > extend_for
			{
				extend_for = len;
			}
		}
		dbg!(&self.span.loc_start, &self.span.loc_end);

		if lines.is_empty() {
			lines.push_str(&
				("?".to_string() + &".".repeat(self.span.loc_end.col.saturating_sub(1)) + "\n")
				.replace("\t", " ")
			);
		}

		format!(
			"\
			{BLUE}--> {GRAY}{file}:{row}:{col}\n\
			{BLUE}    |\n\
			{NORMAL}{lines}\
			{BLUE}    | {spaces}{BOLD}{color}{symbol}{NORMAL}{color} {content}{NORMAL}\
			",
			file = self.span.file_name,
			row = self.span.loc_start.row + 1,
			col = self.span.loc_start.col + 1,
			spaces = " ".repeat(self.span.loc_start.col.min(self.span.loc_end.col.saturating_sub(1))),
			symbol = symbol.repeat(extend_for),
			content = self.content
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
	pub fn instead(vec: Vec<Diagnostic>) -> Self {
		Self {
			before_error: vec,
			after_error: Vec::new(),
			explain_error: false
		}
	}
}

#[macro_export]
macro_rules! diagnostic {
	($level:ident, $span:expr, $content:expr) => {
		crate::errors::Diagnostic {
			level: crate::errors::InfoLevel::$level,
			span: $span,
			content: $content,
		}
	};
}

pub(crate) use diagnostic;

#[macro_export]
/// (span: Span, error: String, info: ErrorInfo)
macro_rules! pb_err {
	($span:expr, $err:expr, $expl:expr) => {
		{
			use crate::errors::diagnostic;
			let e = $expl;
			PunybufError {
				before_error: e.before_error,
				after_error: e.after_error,
				display_error: e.explain_error,
				error: diagnostic!(Error,
					$span.clone(),
					$err
				),
			}
		}
	};
	($span:expr, $err:expr, $($prop_name:ident: $prop:expr),+) => {
		{
			use crate::errors::diagnostic;
			PunybufError {
				error: diagnostic!(Error,
					$span.clone(),
					$err
				),
				$($prop_name: $prop),+,
				..PunybufError::default()
			}
		}
	};
	($span:expr, $err:expr) => {
		PunybufError {
			before_error: vec![],
			after_error: vec![],
			display_error: true,
			error: crate::errors::diagnostic!(Error,
				$span.clone(),
				$err
			),
		}
	};
}

pub(crate) use pb_err;

#[macro_export]
macro_rules! parser_err {
	($span:expr, $string:literal, $($rpt:expr),+) => {
		crate::errors::pb_err!(
			$span.clone(),
			format!($string, $($rpt),+)
		)
	};
	($span:expr, $string:literal) => {
		crate::errors::pb_err!(
			$span.clone(),
			format!($string)
		)
	};
}

pub(crate) use parser_err;