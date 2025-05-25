use std::{env, fs::read_to_string, io, path::Path, rc::Rc};

use crate::{
	errors::{ExtendedErrorExplanation, InfoExplanation, InfoLevel, PunybufError, BOLD, NORMAL, YELLOW},
	lexer::{IncludeHandler, Lexer, Loc, Span, Token},
	pb_err
};

const COMMON: &str = include_str!("../baked/common.pbd");

fn io_err(error: &str) -> io::Error {
	io::Error::other(error)
}
/// Returns `(output_tokens, includes_common)`
// I don't particularly like the lexer being destroyed here, so perhaps Rc<RefCell> wasn't that bad.
// If it ever causes problems, look at fe8a47f.
pub fn tokens_from_file<'a>(file: &'a Path) -> Result<Result<(Vec<Token>, bool), PunybufError>, io::Error> {
	let mut a = FileIncludeHandler {
		root_path: file.parent().ok_or(io::Error::other("cannot find parent directory of a file"))?.into(),
		included: vec![
			(file.to_str().ok_or(io_err("Invalid UTF-8"))?.to_string(), Span::impossible())
		]
	};
	let mut l = lexer_from_file(file, &mut a).map(|x| Box::new(x))?;
	Ok(l.lex().map(|tokens| (tokens, l.includes_common)))
}
fn lexer_from_file<'a>(file: &'a Path, include_handler: &'a mut FileIncludeHandler) -> Result<Lexer<'a, FileIncludeHandler>, io::Error> {
	let content = read_to_string(&file)?;

	let f_str = file.to_str().ok_or(io_err("Invalid UTF-8"))?;

	Ok(Lexer::new(content, f_str, include_handler))
}

pub struct IncludeDisallowed;
impl IncludeHandler for IncludeDisallowed {
	fn handle_include(&mut self, _: String, include_span: Span) -> Result<Vec<Token>, PunybufError> {
		Err(pb_err!(include_span, "include is not allowed here".to_string(), ExtendedErrorExplanation::empty()))
	}
}

struct FileIncludeHandler {
	root_path: Box<Path>,
	included: Vec<(String, Span)>
}

impl IncludeHandler for FileIncludeHandler {
	fn handle_include(&mut self, include_path: String, include_span: Span) -> Result<Vec<Token>, PunybufError> {
		if include_path == "common" {
			if self.included.iter().find(|(i, _)| i == "common").is_some() {
				// Including common multiple times is okay
				return Ok(vec![]);
			}
			self.included.push((include_path, include_span.clone()));
			let mut rust_is_funny = IncludeDisallowed;
			let mut l = Lexer::new(COMMON.to_string(), "<common>", &mut rust_is_funny);
			return l.lex();
		}
		let real_path = self.root_path.join(Path::new(&include_path));

		// unwrapping is fine since this path is from joining two
		// valid utf-8 paths.
		let rp_str = real_path.to_str().unwrap();
		let rp_string = rp_str.to_string();

		// To prevent infinite loops, we store the already-included
		// paths in a Vec, and output a warning if we hit something we
		// included. This makes our includes less powerful than in, say, C,
		// but that's because we don't have defines and stuff and also
		// you shouldn't create libraries of pbd's lol
		for (i_path, i_span) in self.included.iter() {
			if *i_path != rp_string {
				continue;
			}

			let warning = InfoExplanation {
				span: include_span.clone(),
				content: format!("\"{rp_string}\" included here again"),
				level: InfoLevel::Warning
			};

			let expl = if *i_span == Span::impossible() {
				let command_start = format!("$ {} \"", env::args().next().unwrap_or("pbd".to_string()));
				vec![
					InfoExplanation {
						span: Span {
							loc_start: Loc { row: 0, col: command_start.len() },
							loc_end: Loc { row: 0, col: command_start.len() + rp_string.len() },
							file_name: "<shell>".to_string(),
							file_contents: Rc::new(format!("{command_start}{rp_string}\""))
						},
						content: format!("\"{rp_string}\" is the entry point..."),
						level: InfoLevel::Info
					},
					warning
				]
			} else {
				vec![
					InfoExplanation {
						span: i_span.clone(),
						content: format!("\"{rp_string}\" included here first..."),
						level: InfoLevel::Info
					},
					warning
				]
			};

			eprint!("{YELLOW}{BOLD}warning:{NORMAL} \"{rp_string}\" included multiple times - ignored\n");
			for (i, info) in expl.iter().enumerate() {
				if i != 0 { eprint!("\n") }
				eprint!("{}\n", info.explain());
			}

			return Ok(vec![]);
		}

		self.included.push((rp_string, include_span.clone()));

		let mut l = lexer_from_file(&real_path, self).map_err(|err| {
			pb_err!(
				include_span,
				format!("I/O error while including \"{rp_str}\": {err}"),
				ExtendedErrorExplanation::error_and(vec![
					InfoExplanation {
						content: format!("does this file exist?"),
						span: include_span.clone(),
						level: InfoLevel::Tip
					}
				])
			)
		})?;
		match l.lex() {
			Ok(x) => Ok(x),
			Err(mut error) => {
				match error.explanation {
					Some(ref mut expl) => {
						// This only applies to lexer errors, which is very limited
						// in scope, but it's not really that useful anyway...
						expl.after_error.push(InfoExplanation {
							content: format!("...\"{include_path}\" gets included here"),
							span: include_span.clone(),
							level: InfoLevel::Info
						});
					},
					None => {}
				}

				Err(error)
			}
		}
	
	}
}