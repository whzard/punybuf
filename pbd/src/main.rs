use std::{fs::{read_to_string, File}, io::Write, path::{Path, PathBuf}, process::exit};
use clap::{arg, command};

mod files;

mod lexer;
use lexer::Token;

mod errors;
use errors::*;

mod parser;
use parser::Parser;

mod flattener;
use flattener::{flatten, PunybufDefinition};

mod validator;

mod resolver;
use resolver::LayerResolver;

mod converter;

mod rust_codegen;
use rust_codegen::RustCodegen;

mod binary_compat;


fn main() {
	let args = command!()
		.about("Generate code or IR from a Punybuf Definition file.")
		.arg(arg!(<INPUT> "The .pbd definition file").required(true))
		.arg(arg!(-q --quiet "Do not print JSON into stdout"))
		.arg(arg!(-l --loud "Do print JSON into stdout, overrides -q"))
		.arg(arg!(-o --out... <OUT> "Output - only .rs, .json files supported. Implies -q").num_args(1..))
		.arg(arg!(-c --compat <JSON> 
			"Check binary compatibility with the previous version (json file). \
			If compatible, overwrite the file, otherwise, error."
		))
		.arg(arg!(-d --"dry-run" "Do not write anything to the filesystem."))
		.arg(arg!(--verbose "Be verbose. Will print a lot of unnecessary things."))
		.arg(arg!(--"no-resolve" "Skip `@resolve`-ing aliases."))
		.arg(arg!(--"rust:tokio" "Generate async rust code for tokio. Affects only `.rs` files from --out."))
		.get_matches()
	;

	let file = args.get_one::<String>("INPUT").unwrap();
	let out = args.get_many::<String>("out").map(|x| x.collect::<Vec<_>>()).unwrap_or(vec![]);
	let quiet = (args.get_flag("quiet") || !out.is_empty()) && !args.get_flag("loud");
	let dry = args.get_flag("dry-run");
	let verbose = args.get_flag("verbose");
	let resolve = !args.get_flag("no-resolve");
	let check_binary = args.get_one::<String>("compat");

	macro_rules! verboseln {
		($($meow:expr),+) => {
			if verbose { eprintln!($($meow),+) }
		};
	}

	verboseln!("File: {file}");
	let result = (|| -> Result<(), String> {
		let (tokens, includes_common) = files::tokens_from_file(Path::new(file))
			.map_err(|e| e.to_string())?
			.map_err(|e| e.to_string())?;

		verboseln!("Tokens: {:?}", tokens);

		let mut p = Parser::new(&tokens);
		let decls = p.parse().map_err(|e| e.to_string())?;
		verboseln!("Decls: {:?}", decls);

		let mut def: PunybufDefinition = flatten(decls, includes_common).map_err(|e| e.to_string())?;
		verboseln!("Definition: {:?}", def);
		def.validate().map_err(|e| e.to_string())?;

		LayerResolver::new(resolve).resolve(&mut def);

		if let Some(compat) = check_binary {
			let json = read_to_string(compat).map_err(|e| e.to_string())?;
			binary_compat::BinaryCompat.check(&json, &def).map_err(|e| e.to_string())?;
		}

		for out_file in out {
			#[allow(unused_assignments)] // idk why it does that
			let mut file_type = "unknown";
			let generated = if out_file.ends_with(".rs") {
				file_type = "Rust";
				RustCodegen::new(args.get_flag("rust:tokio")).codegen(&def)

			} else if out_file.ends_with(".json") {
				file_type = "JSON";
				converter::convert_full_definition(&def)

			} else {
				return Err(format!(
					"can't output a file `{out_file}` - file type not supported\n  \
					perhaps you wanted to pipe the output from this command into another?"
				));
			};

			if dry {
				eprintln!("would've written to the file: {BLUE}{BOLD}{out_file}{NORMAL}, but {RED}--dry-run{NORMAL} was specified");
				continue
			}

			let mut file = File::create(out_file).map_err(|e| e.to_string())?;
			file.write_all(generated.as_bytes()).map_err(|e| e.to_string())?;
			eprintln!("{GREEN}{BOLD}generated:{NORMAL} {out_file} {GRAY}({file_type}){NORMAL}");
		}

		if !quiet {
			println!("{}", converter::convert_full_definition(&def));
		}

		Ok(())
	})();

	if let Err(e) = result {
		eprintln!("{RED}{BOLD}error:{NORMAL} {e}");
		exit(1)
	}
}