use crate::{errors::PunybufError, flattener::PunybufDefinition};

pub struct BinaryCompat;

impl BinaryCompat {
	pub fn check(&self, prev_json: &str, next: &PunybufDefinition) -> Result<(), PunybufError> {
		
		Ok(())
	}
}