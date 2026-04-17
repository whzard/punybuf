mod lexer;
mod binary_compat;
mod converter;
mod errors;
mod files;
mod parser;
mod resolver;
mod flattener;
mod validator;
mod rust_codegen;

use std::{io, path::{Path}};

use crate::{errors::PunybufError, flattener::PunybufDefinition, parser::{Declaration, Parser}, resolver::LayerResolver};

pub struct PunybufParser;

pub use crate::converter::convert_full_definition;
pub use crate::rust_codegen::RustCodegen;

pub struct Parsed {
	declarations: Vec<Declaration>,
	includes_common: bool
}

impl PunybufParser {
	pub fn parse_file<P: AsRef<Path>>(path: P) -> Result<Result<Parsed, PunybufError>, io::Error> {
		let (tokens, includes_common) = match files::tokens_from_file(path.as_ref()) {
			Ok(v) => match v {
				Ok(v) => v,
				Err(e) => return Ok(Err(e))
			}
			Err(e) => return Err(e)
		};
		
		let declarations = match Parser::new(&tokens).parse() {
			Ok(v) => v,
			Err(e) => return Ok(Err(e))
		};

		Ok(Ok(Parsed { declarations, includes_common }))
	}
}

impl Parsed {
	pub fn includes_common(&self) -> bool {
		self.includes_common
	}
	/// Resolves and validates the token tree
	pub fn resolve(self, should_resolve_aliases: bool) -> Result<PunybufDefinition, PunybufError> {
		let mut definition = flattener::flatten(self.declarations, self.includes_common)?;
		definition.validate()?;
		LayerResolver::new(should_resolve_aliases).resolve(&mut definition);
		Ok(definition)
	}
}