mod lexer;
mod binary_compat;
mod converter;
mod errors;
mod files;
mod parser;
mod resolver;
mod flattener;
mod validator;
mod codegen;

use std::{io, path::{Path}};

pub use crate::{
	errors::PunybufError, flattener::PunybufDefinition,
	parser::{Parser}, resolver::LayerResolver,
	converter::ConvertJSON,
	lexer::{IncludeDisallowed, IncludeHandler},
	codegen::*
};


use crate::{
	lexer::Lexer, parser::Declaration,
};

/// The parsed (but not yet validated) Punybuf definition
pub struct Parsed {
	declarations: Vec<Declaration>,
	includes_common: bool
}

pub struct PunybufParser;
impl PunybufParser {
	/// Parse the file located at `path`, honoring `include`s relative to that path
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
	/// Parse the contents of the string, without caring about where it
	/// came from. `include`s in the source code are not allowed.
	pub fn parse_string(str: String) -> Result<Parsed, PunybufError> {
		Self::parse(str, "<source>".into(), &mut IncludeDisallowed)
	}
	/// Parse with custom parameters. When the `include` keyword is
	/// encountered, the [`IncludeHandler::handle_include`] method
	/// will be called. If you do not wish to handle `include`s, pass
	/// [`IncludeDisallowed`] as the `include_handler`.
	/// The `file_name` parameter is needed only for diagnostics.
	pub fn parse(
		contents: String, file_name: String,
		include_handler: &mut impl IncludeHandler
	)
		-> Result<Parsed, PunybufError>
	{
		let mut lexer = Lexer::new(contents, file_name, include_handler);
		let tokens = lexer.lex()?;
		let declarations = Parser::new(&tokens).parse()?;
		Ok(Parsed { declarations, includes_common: lexer.includes_common })
	}
}

impl Parsed {
	/// Whether the definition contains an `include common` somewhere.
	pub fn includes_common(&self) -> bool {
		self.includes_common
	}
	/// Resolve and validate the definition
	pub fn resolve(self, should_resolve_aliases: bool) -> Result<PunybufDefinition, PunybufError> {
		let mut definition = flattener::flatten(self.declarations, self.includes_common)?;
		definition.validate()?;
		LayerResolver::new(should_resolve_aliases).resolve(&mut definition);
		Ok(definition)
	}
}