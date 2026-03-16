use crate::{errors::PunybufError, flattener::PunybufDefinition};

pub struct BinaryCompat<'a> {
	prev_json: &'a str,
	next: &'a PunybufDefinition,
}

impl<'a> BinaryCompat<'a> {
	pub fn new(prev_json: &'a str, next: &'a PunybufDefinition) -> Result<Self, String> {
		Ok(Self {
			prev_json, next
		})
	}
	pub fn check(&self) -> Result<(), PunybufError> {
		
		Ok(())
	}
}