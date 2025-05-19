use std::{cell::RefCell, env, fs::read_to_string, io, path::Path, rc::Rc};

use crate::{
	errors::{ExtendedErrorExplanation, InfoExplanation, InfoLevel, PunybufError, YELLOW, BOLD, NORMAL},
	lexer::{Lexer, Loc, Span},
	pb_err
};

const COMMON: &str = include_str!("../baked/common.pbd");

fn io_err(error: &str) -> io::Error {
	io::Error::other(error)
}

pub fn lexer_from_file<'a>(file: &'a Path) -> Result<Lexer<'a>, io::Error> {
	lexer_from_file_internal(file, &mut Rc::new(RefCell::new(vec![
		(file.to_str().ok_or(io_err("Invalid UTF-8"))?.to_string(), Span::impossible())
	])))
}
fn lexer_from_file_internal<'a>(file: &'a Path, included: &mut Rc<RefCell<Vec<(String, Span)>>>) -> Result<Lexer<'a>, io::Error> {
	let mut included = included.clone();

	let parent = file.parent().ok_or(io_err("Invalid include path"))?;
	let content = read_to_string(&file)?;

	let f_str = file.to_str().ok_or(io_err("Invalid UTF-8"))?;

	Ok(Lexer::new(content, f_str, Box::new(move |inlcude_file_name, include_span| {
		if inlcude_file_name == "common" {
			if included.borrow().iter().find(|(i, _)| i == "common").is_some() {
				return Ok(vec![]);
			}
			included.borrow_mut().push((inlcude_file_name, include_span.clone()));
			let mut l = Lexer::new(COMMON.to_string(), "<common>", Box::new(|_, _| {
				Err(pb_err!(include_span, "<common> cannot include".to_string(), ExtendedErrorExplanation::empty()))
			}));
			return l.lex();
		}
		let real_path = parent.join(Path::new(&inlcude_file_name));

		// unwrapping is fine since this path is from joining two
		// valid utf-8 paths.
		let rp_str = real_path.to_str().unwrap();
		let rp_string = rp_str.to_string();

		// To prevent infinite loops, we store the already-included
		// paths in a Vec, and output a warning if we hit something we
		// included. This makes our includes less powerful than in, say, C,
		// but that's because we don't have defines and stuff and also
		// you shouldn't create libraries of pbd's lol
		for (i_path, i_span) in included.borrow().iter() {
			if *i_path != rp_string {
				continue;
			}

			let warning = InfoExplanation {
				span: include_span.clone(),
				content: format!("\"{rp_string}\" included here again"),
				level: InfoLevel::Warning
			};

			let expl = if *i_span == Span::impossible() {
				let command_start = format!("$ {} \"", env::args().next().unwrap_or("punybuf".to_string()));
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

		included.borrow_mut().push((rp_string, include_span.clone()));

		let mut l = lexer_from_file_internal(&real_path, &mut included).map_err(|err| {
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
							content: format!("...\"{inlcude_file_name}\" gets included here"),
							span: include_span.clone(),
							level: InfoLevel::Info
						});
					},
					None => {}
				}

				Err(error)
			}
		}
	})))
}
