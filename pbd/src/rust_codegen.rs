use crate::flattener::{PBCommandArg, PBCommandDef, PBEnumVariant, PBField, PBTypeDef, PBTypeRef, PunybufDefinition};

const TO_MAP: &str = r#"
    fn to_map_allow_duplicates(self) -> (std::collections::HashMap<K, V>, bool) {
        let mut hm = std::collections::HashMap::new();
        let mut duplicates = false;
        for pair in self {
            if hm.insert(pair.key, pair.value).is_some() {
                duplicates = true;
            }
        }
        (hm, duplicates)
    }
    fn from_map(map: std::collections::HashMap<K, V>) -> Self {
        let mut this = Self::new();
        for (key, value) in map.into_iter() {
            this.push(KeyPair { key, value });
        }
        this
    }
"#;

const HASH_MAP_CONVERTIBLE: &str = r#"
// Because of Rust's orphan rules, we can't put this in the punybuf_common crate.

pub struct DuplicateKeysFound;
pub trait HashMapConvertible<K, V>: Sized {
    /// Converts the value to a `HashMap`, overriding duplicate keys.  
    /// Returns the resulting hashmap and a boolean indicating whether any duplicate keys were found
    fn to_map_allow_duplicates(self) -> (std::collections::HashMap<K, V>, bool);

    /// Returns an error if there were any duplicate keys in the Map
    fn try_to_map(self) -> Result<std::collections::HashMap<K, V>, DuplicateKeysFound> {
        let (map, duplicates_found) = self.to_map_allow_duplicates();
        if !duplicates_found {
            Ok(map)
        } else {
            Err(DuplicateKeysFound)
        }
    }
    fn from_map(map: std::collections::HashMap<K, V>) -> Self;
}
"#;

pub struct RustCodegen {
	use_tokio: bool,
	uses_common: bool,
	gen_docs: bool,
	buffer: String,
}

macro_rules! appendf {
	($s:ident, $x:literal, $($rpt:expr),*) => {
		$s.buffer.push_str(&format!($x, $($rpt),*))
	};
	($s:ident, $x:literal) => {
		$s.buffer.push_str(&format!($x))
	};
}

impl RustCodegen {
	pub fn new(use_tokio: bool, gen_docs: bool) -> Self {
		Self {
			use_tokio,
			uses_common: true,
			gen_docs,
			buffer: String::new(),
		}
	}
	fn get_fn(&self) -> &str {
		if self.use_tokio {
			"async fn"
		} else {
			"fn"
		}
	}
	fn write(&self) -> &str {
		if self.use_tokio {
			"AsyncWriteExt + Unpin + Send"
		} else {
			"io::Write"
		}
	}
	fn read(&self) -> &str {
		if self.use_tokio {
			"AsyncReadExt + Unpin + Send"
		} else {
			"io::Read"
		}
	}
	fn read_exact(&self, arg: &str) -> String {
		if self.use_tokio {
			format!("read_exact({}).await?", arg)
		} else {
			format!("read_exact({})?", arg)
		}
	}
	/* /// Gets a fully qualified reference name
	fn get_ref(&self, refr: &PBTypeRef) -> String {
		if self.uses_common {
			match refr.reference.as_str() {
				s @ (
					"U8" | "U16" | "U32" | "U64" | "I32" | "I64" | "F32" | "F64"
				) => return s.to_ascii_lowercase(),
				s @ (
					"String" | "Bytes" | "UInt"
				) => return s.to_string(),
				"Array" => return "Vec".to_string(),
				_ => {}
			}
		}
		if refr.is_highest_layer || refr.resolved_layer.is_none() {
			refr.reference.clone()
		} else {
			format!("{}Layer{}", refr.reference, refr.resolved_layer.unwrap())
		}
	} */
	/// Generates a reference, including generics
	fn gen_reference(&self, refr: &PBTypeRef, turbofish: bool) -> String {
		if self.uses_common {
			match refr.reference.as_str() {
				s @ (
					"U8" | "U16" | "U32" | "U64" | "I32" | "I64" | "F32" | "F64"
				) => return s.to_ascii_lowercase(),
				s @ (
					"String" | "Bytes" | "UInt"
				) => return s.to_string(),
				_ => {}
			}
		}
		let mut result = if self.uses_common && refr.reference == "Array" {
			"Vec".to_string()
		} else if refr.is_highest_layer || refr.resolved_layer.is_none() {
			refr.reference.clone()
		} else {
			format!("{}Layer{}", refr.reference, refr.resolved_layer.unwrap())
		};
		if refr.generics.is_empty() {
			return result;
		}

		if turbofish {
			result.push_str("::<");
		} else {
			result.push('<');
		}

		for (i, gen) in refr.generics.iter().enumerate() {
			if i != 0 {
				result.push_str(", ");
			}
			result.push_str(&self.gen_reference(gen, turbofish));
		}
		result.push('>');
		return result;
	}
	fn get_command_name(&self, cmd: &PBCommandDef) -> String {
		if cmd.is_highest_layer {
			cmd.name.clone()
		} else {
			format!("{}Layer{}", cmd.name, cmd.layer)
		}
	}
	fn get_type_name(&self, tp: &PBTypeDef) -> String {
		let mut result = if tp.is_highest_layer() {
			tp.get_name().0.to_string()
		} else {
			format!("{}Layer{}", tp.get_name().0, tp.get_layer())
		};
		if !tp.get_generics().0.is_empty() {
			let generics = tp.get_generics().0;
			result.push('<');
			for (i, g) in generics.iter().enumerate() {
				if i != 0 {
					result.push_str(", ");
				}
				result.push_str(&g);
			}
			result.push('>');
		}
		result
	}
	fn get_type_impl_generics(&self, tp: &PBTypeDef) -> String {
		if tp.get_generics().0.is_empty() {
			"".to_string()
		} else {
			let mut result = String::new();
			let generics = tp.get_generics().0;
			result.push('<');
			for (i, g) in generics.iter().enumerate() {
				if i != 0 {
					result.push_str(", ");
				}
				result.push_str(&format!("{}: PBType", g));
			}
			result.push('>');
			result
		}
	}
	fn get_command_err(&self, cmd: &PBCommandDef) -> String {
		if cmd.is_highest_layer {
			format!("{}Error", cmd.name)
		} else {
			format!("{}Layer{}Error", cmd.name, cmd.layer)
		}
	}
	fn maybe_await(&self) -> &str {
		if self.use_tokio {
			".await"
		} else {
			""
		}
	}
	fn gen_command_enums(&mut self, def: &PunybufDefinition) {
		/* appendf!(self, "pub struct CommandId(u32);\n");
		appendf!(self, "impl CommandId {{\n");
		appendf!(self, "    /// This function **panics** if the passed CommandId is invalid\n");
		appendf!(self, "    pub fn new(id: u32) -> Self {{\n");
		appendf!(self, "        match id {{\n");
		for (i, cmd) in def.commands.iter().enumerate() {
			if cmd.attrs.contains_key("@rust:ignore") {
				continue;
			}
			if i == 0 {
				appendf!(self, "            ");
			} else {
				appendf!(self, " |\n            ");
			}
			appendf!(self, "{}", cmd.command_id);
		}
		appendf!(self, "\n");
		appendf!(self, "                => Self(id),\n");
		appendf!(self, r#"            _ => panic!("invalid command id")"#);
		appendf!(self, "\n");
		appendf!(self, "        }}\n"); // match
		appendf!(self, "    }}\n"); // fn new()

		appendf!(self, "    /// Doesn't check whether `id` is a supported command ID.  \n");
		appendf!(self, "    /// This isn't unsafe, but is likely to cause errors later.\n");
		appendf!(self, "    pub fn new_unchecked(id: u32) -> Self {{\n");
		appendf!(self, "        Self(id)\n");
		appendf!(self, "    }}\n"); // fn new_unchecked()
		appendf!(self, "}}\n"); // impl */

		appendf!(self, "/// This enum contains all possible commands in the RPC definition.\n");
		appendf!(self, "#[derive(Debug)]\n");
		appendf!(self, "pub enum Command {{\n");
		for cmd in &def.commands {
			if cmd.attrs.contains_key("@rust:ignore") {
				continue;
			}
			appendf!(self, "    {}({}),\n", self.get_command_name(cmd), self.get_command_name(cmd));
		}
		appendf!(self, "}}\n"); // enum Command

		appendf!(self, "impl Command {{\n");
		appendf!(self, "    pub {} deserialize_command<R: {}>(r: &mut R) -> Result<Self, io::Error> {{\n", self.get_fn(), self.read());
		appendf!(self, "        let mut id = [0; 4];\n");
		appendf!(self, "        r.{};\n", self.read_exact("&mut id"));
		appendf!(self, "        let id = u32::from_be_bytes(id);\n");
		appendf!(self, "        Ok(match id {{\n");
		for cmd in &def.commands {
			if cmd.attrs.contains_key("@rust:ignore") {
				continue;
			}
			appendf!(self,
				"            {} => Self::{}({}::deserialize(r){}?),\n",
				cmd.command_id, self.get_command_name(cmd), self.get_command_name(cmd), self.maybe_await()
			);
		}
		appendf!(self, r#"            _ => Err(io::Error::other("Invalid or unsupported command ID"))?"#);
		appendf!(self, "\n");
		appendf!(self, "        }})\n"); // match
		appendf!(self, "    }}\n"); // fn deserialize_command()

		appendf!(self, "    pub fn id(&self) -> u32 {{\n");
		appendf!(self, "        match self {{\n");
		for cmd in &def.commands {
			if cmd.attrs.contains_key("@rust:ignore") {
				continue;
			}
			appendf!(self, "            Self::{}(_) => {},\n", self.get_command_name(cmd), cmd.command_id);
		}
		appendf!(self, "        }}\n"); // match
		appendf!(self, "    }}\n"); // fn id()

		appendf!(self, "    pub fn attributes(&self) -> &'static [(&'static str, Option<&'static str>)] {{\n");
		appendf!(self, "        match self {{\n");
		for cmd in &def.commands {
			if cmd.attrs.contains_key("@rust:ignore") {
				continue;
			}
			appendf!(self, "            Self::{}(_) => {}::attributes(),\n", self.get_command_name(cmd), self.get_command_name(cmd));
		}
		appendf!(self, "        }}\n"); // match
		appendf!(self, "    }}\n"); // fn attributes()

		appendf!(self, "    pub fn required_capability(&self) -> Option<&'static str> {{\n");
		appendf!(self, "        match self {{\n");
		for cmd in &def.commands {
			if cmd.attrs.contains_key("@rust:ignore") {
				continue;
			}
			appendf!(self, "            Self::{}(_) => {}::required_capability(),\n", self.get_command_name(cmd), self.get_command_name(cmd));
		}
		appendf!(self, "        }}\n"); // match
		appendf!(self, "    }}\n"); // fn required_capability()
		appendf!(self, "}}\n\n"); // impl Command


		appendf!(self, "/// This enum contains all possible command return types in the RPC definition.\n");
		appendf!(self, "#[derive(Debug)]\n");
		appendf!(self, "pub enum CommandReturn {{\n");
		for cmd in &def.commands {
			if cmd.attrs.contains_key("@rust:ignore") {
				continue;
			}
			appendf!(self, "    {}({}),\n", self.get_command_name(cmd), self.gen_reference(&cmd.ret, false));
		}
		appendf!(self, "}}\n"); // enum CommandReturn


		appendf!(self, "impl CommandReturn {{\n");
		appendf!(self, "    pub {} deserialize_return<R: {}>(id: u32, r: &mut R) -> Result<Self, io::Error> {{\n", self.get_fn(), self.read());
		appendf!(self, "        Ok(match id {{\n");
		for cmd in &def.commands {
			if cmd.attrs.contains_key("@rust:ignore") {
				continue;
			}
			appendf!(self,
				"            {} => Self::{}({}::deserialize(r){}?),\n",
				cmd.command_id, self.get_command_name(cmd), self.gen_reference(&cmd.ret, true), self.maybe_await()
			);
		}
		appendf!(self, r#"            _ => Err(io::Error::other("Invalid or unsupported command ID"))?"#);
		appendf!(self, "\n");
		appendf!(self, "        }})\n"); // match
		appendf!(self, "    }}\n"); // fn deserialize_return()
		appendf!(self, "}}\n\n"); // impl CommandReturn

		appendf!(self, "/// This enum contains all possible command error types in the RPC definition.\n");
		appendf!(self, "#[derive(Debug)]\n");
		appendf!(self, "pub enum CommandError {{\n");
		for cmd in &def.commands {
			if cmd.attrs.contains_key("@rust:ignore") {
				continue;
			}
			appendf!(self, "    {}({}),\n", self.get_command_name(cmd), self.get_command_err(cmd));
		}
		appendf!(self, "}}\n"); // enum CommandError

		appendf!(self, "impl CommandError {{\n");
		appendf!(self, "    pub {} deserialize_error<R: {}>(id: u32, r: &mut R) -> Result<Self, io::Error> {{\n", self.get_fn(), self.read());
		appendf!(self, "        Ok(match id {{\n");
		for cmd in &def.commands {
			if cmd.attrs.contains_key("@rust:ignore") {
				continue;
			}
			appendf!(self,
				"            {} => Self::{}({}::deserialize(r){}?),\n",
				cmd.command_id, self.get_command_name(cmd), self.get_command_err(cmd), self.maybe_await()
			);
		}
		appendf!(self, r#"            _ => Err(io::Error::other("Invalid or unsupported command ID"))?"#);
		appendf!(self, "\n");
		appendf!(self, "        }})\n"); // match
		appendf!(self, "    }}\n"); // fn deserialize_error()
		appendf!(self, "}}\n\n"); // impl CommandError
	}
	fn gen_fields(&mut self, fields: &Vec<PBField>) {
		for field in fields {
			if let Some(flags) = &field.flags {
				for flag in flags {
					self.gen_doc(&flag.doc, 1);
					appendf!(self, "    pub {}: ", flag.name);
					if let Some(val) = &flag.value {
						appendf!(self, "Option<{}>,", self.gen_reference(val, false));
					} else {
						appendf!(self, "bool,");
					}
					appendf!(self, " // Flag of `{}`\n", field.name);
				}
			} else {
				// Flag fields are an implementation detail and we would like
				// to hide it (so that the struct is easily constructable)
				self.gen_doc(&field.doc, 1);
				appendf!(self, "    pub {}: {},\n", field.name, self.gen_reference(&field.value, false));
			}
		}
	}
	fn gen_variants(&mut self, variants: &Vec<PBEnumVariant>) {
		for variant in variants {
			self.gen_doc(&variant.doc, 1);
			appendf!(self, "    {}", variant.name);
			if let Some(val) = &variant.value {
				appendf!(self, "({})", self.gen_reference(val, false))
			}
			appendf!(self, ",\n")
		}
	}
	/* fn gen_flags_type(&self, flags_type: &PBTypeRef) -> &str {
		
	} */
	fn gen_serialize_fields(&mut self, fields: &Vec<PBField>, extensible: bool) {
		let mut has_extensions = false;
		for field in fields {
			if let Some(flags) = &field.flags {
				appendf!(self, "        // If you get an error here, this type doesn't support flags.\n");
				appendf!(self, "        let mut flags: {} = 0.try_into().unwrap();\n", self.gen_reference(&field.value, false));
				for (i, flag) in flags.iter().enumerate() {
					if flag.value.is_some() {
						appendf!(self, "        if self.{}.is_some() {{ flags |= 1 << {i} }}\n", flag.name);
					} else {
						appendf!(self, "        if self.{} {{ flags |= 1 << {i} }}\n", flag.name);
					}
				}
				appendf!(self, "        flags.serialize(w){}?;\n", self.maybe_await());
				for flag in flags {
					if flag.value.is_none() { continue }
					if flag.attrs.contains_key("@extension") {
						has_extensions = true;
						continue;
					}

					appendf!(self, "        if let Some(ref v) = self.{} {{\n", flag.name);
					appendf!(self, "            v.serialize(w){}?;\n", self.maybe_await());
					appendf!(self, "        }}\n");
				}
			} else {
				appendf!(self, "        self.{}.serialize(w){}?;\n", field.name, self.maybe_await());
			}
		}
		if extensible && has_extensions {
			appendf!(self, "        let real_w = w;\n");
			appendf!(self, "        let mut bytes = Bytes(Vec::new());\n");
			appendf!(self, "        let w = &mut bytes.0;\n");
			// Probably better to do this with a temporary Vec
			for field in fields {
				let Some(flags) = &field.flags else { continue };
				for flag in flags {
					if flag.value.is_none() || !flag.attrs.contains_key("@extension") {
						continue;
					}

					appendf!(self, "        if let Some(ref v) = self.{} {{\n", flag.name);
					appendf!(self, "            v.serialize(w){}?;\n", self.maybe_await());
					appendf!(self, "        }}\n");
				}
			}
			appendf!(self, "        bytes.serialize(real_w){}?;\n", self.maybe_await());
		} else if extensible {
			appendf!(self, "        UInt(0).serialize(w){}?;\n", self.maybe_await());
		}
	}
	fn gen_deserialize_fields(&mut self, fields: &Vec<PBField>, extensible: bool) {
		for field in fields {
			appendf!(self, "        let field_{} = {}::deserialize(r){}?;\n",
				field.name, self.gen_reference(&field.value, true),
				self.maybe_await()
			);
			if let Some(flags) = &field.flags {
				for (i, flag) in flags.iter().enumerate() {
					if flag.attrs.contains_key("@extension") {
						continue;
					}
					if let Some(val) = &flag.value {
						appendf!(self, "        let flag_{} = if (field_{} & (1 << {i})) != 0 {{\n", flag.name, field.name);
						appendf!(self, "            Some({}::deserialize(r){}?)\n", self.gen_reference(val, true), self.maybe_await());
						appendf!(self, "        }} else {{ None }};\n");
					} else {
						appendf!(self, "        let flag_{} = (field_{} & (1 << {i})) != 0;\n", flag.name, field.name);
					}
				}
			}
		}
		if extensible {
			appendf!(self, "        let mut _extension_bytes = Bytes::deserialize(r){}?;\n", self.maybe_await());
			appendf!(self, "        let _extension_reader = &mut &_extension_bytes.0[..];\n");
			for field in fields {
				let Some(flags) = &field.flags else { continue };
				for (i, flag) in flags.iter().enumerate() {
					if !flag.attrs.contains_key("@extension") {
						continue;
					}

					if let Some(val) = &flag.value {
						appendf!(self, "        let flag_{} = if (field_{} & (1 << {i})) != 0 {{\n", flag.name, field.name);
						appendf!(self, "            Some({}::deserialize(_extension_reader){}?)\n", self.gen_reference(val, true), self.maybe_await());
						appendf!(self, "        }} else {{ None }};\n");

					} else {
						appendf!(self, "        let flag_{} = (field_{} & (1 << {i})) != 0;\n", flag.name, field.name);
					}
				}
			}
		}
		appendf!(self, "        Ok(Self {{\n");
		for field in fields {
			if let Some(flags) = &field.flags {
				for flag in flags {
					appendf!(self, "            {}: flag_{},\n", flag.name, flag.name);
				}
			} else {
				// We don't want to expose the actual flags value in the struct
				appendf!(self, "            {}: field_{},\n", field.name, field.name);
			}
		}
		appendf!(self, "        }})\n");
	}
	fn gen_serialize_variants(&mut self, variants: &Vec<PBEnumVariant>) {
		for variant in variants {
			appendf!(self, "            Self::{}", variant.name);
			if variant.value.is_some() {
				appendf!(self, "(value)");
			}
			appendf!(self, " => {{\n");
			appendf!(self, "                {}u8.serialize(w){}?;\n", variant.discriminant, self.maybe_await());
			if variant.attrs.contains_key("@extension") {
				if variant.value.is_some() {
					appendf!(self, "                // Extension:\n");
					appendf!(self, "                let real_w = w;\n");
					appendf!(self, "                let mut bytes = Bytes(Vec::new());\n");
					appendf!(self, "                let w = &mut bytes.0;\n");
				} else {
					appendf!(self, "                // Skipped extension:\n");
					appendf!(self, "                UInt(0).serialize(w){}?;\n", self.maybe_await());
				}
			}
			if let Some(_) = &variant.value {
				appendf!(self, "                value.serialize(w){}?;\n", self.maybe_await());
			}
			if variant.attrs.contains_key("@extension") && variant.value.is_some() {
				appendf!(self, "                bytes.serialize(real_w){}?;\n", self.maybe_await());
			}
			appendf!(self, "            }}\n");
		}
	}
	fn gen_deserialize_variants(&mut self, variants: &Vec<PBEnumVariant>) {
		let mut default_variant = None;
		for variant in variants {
			if variant.attrs.contains_key("@default") {
				default_variant = Some(variant);
			}
			appendf!(self, "            {} => {{\n", variant.discriminant);
			if variant.attrs.contains_key("@extension") {
				appendf!(self, "                _ = UInt::deserialize(r);\n");
			}
			if let Some(refr) = &variant.value {
				appendf!(self, "                Self::{}({}::deserialize(r){}?)\n", variant.name, self.gen_reference(refr, true), self.maybe_await());
			} else {
				appendf!(self, "                Self::{}\n", variant.name);
			}
			appendf!(self, "            }}\n");
		}
		if let Some(default_variant) = default_variant {
			appendf!(self, "            _ => {{\n");
			appendf!(self, "                _ = Bytes::deserialize(r){}?;\n", self.maybe_await());
			appendf!(self, "                Self::{}\n", default_variant.name);
			appendf!(self, "            }}\n");
		} else {
			appendf!(self, "            _ => {{\n");
			appendf!(self, "                Err(io::Error::other(\"Unknown enum discriminant; enum is not extensible\"))?\n");
			appendf!(self, "            }}\n");
		}
	}
	fn gen_doc(&mut self, doc: &str, indent: usize) {
		if !self.gen_docs || doc == "" {
			return;
		}
		for line in doc.lines() {
			appendf!(self, "{}", "    ".repeat(indent));
			appendf!(self, "/// {}\n", line);
		}
	}
	fn gen_commands(&mut self, def: &PunybufDefinition) {
		for cmd in &def.commands {
			if cmd.attrs.contains_key("@rust:ignore") {
				continue;
			}
			self.gen_doc(&cmd.doc, 0);
			appendf!(self, "#[derive(Debug)]\n");
			appendf!(self, "pub struct {}", self.get_command_name(cmd));
			match &cmd.argument {
				PBCommandArg::None => {
					appendf!(self, ";\n")
				}
				PBCommandArg::Ref(refr) => {
					appendf!(self, "(pub {});\n", self.gen_reference(refr, false))
				}
				PBCommandArg::Struct { fields } => {
					if fields.is_empty() {
						appendf!(self, ";\n");
					} else {
						appendf!(self, " {{\n");
						self.gen_fields(fields);
						appendf!(self, "}}\n");
					}
				}
			}
			appendf!(self, "impl PBCommand for {} {{\n", self.get_command_name(cmd));
			appendf!(self, "    const MIN_SIZE: usize = 0; // TODO\n");
			appendf!(self, "    type Error = {};\n", self.get_command_err(cmd));
			appendf!(self, "    type Return = {};\n", self.gen_reference(&cmd.ret, false));
			appendf!(self, "    fn id() -> u32 {{ {} }}\n", cmd.command_id);
			if !cmd.attrs.is_empty() {
				appendf!(self, "    fn attributes() -> &'static [(&'static str, Option<&'static str>)] {{ &[\n");
				for (name, value) in &cmd.attrs {
					appendf!(self, "        ({name:?}, {value:?}),\n");
				}
				appendf!(self, "    ] }}\n"); // attributes
			}
			if let Some(Some(cap)) = cmd.attrs.get("@capability") {
				appendf!(self, "    fn required_capability() -> Option<&'static str> {{ \n");
				appendf!(self, "        Some(&{cap:?})\n");
				appendf!(self, "    }}\n"); // required_capability
			}
			if cmd.ret.reference == "Void" {
				appendf!(self, "    fn is_void() -> bool {{ true }}\n");
			}
			appendf!(self, "    {} serialize_self<W: {}>(&self, w: &mut W) -> io::Result<()> {{\n", self.get_fn(), self.write());
			match &cmd.argument {
				PBCommandArg::None => {},
				PBCommandArg::Ref(_) => {
					appendf!(self, "        self.0.serialize(w){}?;\n", self.maybe_await());
				},
				PBCommandArg::Struct { fields } => self.gen_serialize_fields(fields, !cmd.attrs.contains_key("@sealed")),
			}
			appendf!(self, "        Ok(())\n");
			appendf!(self, "    }}\n"); // serialize_self
			appendf!(self, "    {} deserialize<R: {}>(r: &mut R) -> io::Result<Self> {{\n", self.get_fn(), self.read());
			match &cmd.argument {
				PBCommandArg::None => {},
				PBCommandArg::Ref(refr) => {
					appendf!(self, "        Self({}::deserialize(r){}?)\n", self.gen_reference(refr, true), self.maybe_await());
				},
				PBCommandArg::Struct { fields } => self.gen_deserialize_fields(fields, !cmd.attrs.contains_key("@sealed")),
			}
			appendf!(self, "    }}\n"); // deserialize
			appendf!(self, "}}\n\n"); // impl PBCommand

			appendf!(self, "#[derive(Debug)]\n");
			appendf!(self, "pub enum {} {{\n", self.get_command_err(cmd));
			appendf!(self, "    UnexpectedError(String),\n");
			self.gen_variants(&cmd.err);
			appendf!(self, "}}\n"); // enum
			appendf!(self, "impl PBType for {} {{\n", self.get_command_err(cmd));
			appendf!(self, "    const MIN_SIZE: usize = 0; // TODO\n");
			appendf!(self, "    {} serialize<W: {}>(&self, w: &mut W) -> io::Result<()> {{\n", self.get_fn(), self.write());
			appendf!(self, "        match self {{\n");
			appendf!(self, "            Self::UnexpectedError(x) => {{ 0u8.serialize(w){}?; x.serialize(w){}?; }}\n", self.maybe_await(), self.maybe_await());
			self.gen_serialize_variants(&cmd.err);
			appendf!(self, "        }}\n"); // match
			appendf!(self, "        Ok(())\n");
			appendf!(self, "    }}\n"); // fn serialize
			appendf!(self, "    {} deserialize<R: {}>(r: &mut R) -> io::Result<Self> {{\n", self.get_fn(), self.read());
			appendf!(self, "        let discriminant = u8::deserialize(r){}?;\n", self.maybe_await());
			appendf!(self, "        Ok(match discriminant {{\n");
			appendf!(self, "            0 => {{ Self::UnexpectedError(String::deserialize(r){}?) }}\n", self.maybe_await());
			self.gen_deserialize_variants(&cmd.err);
			appendf!(self, "        }})\n"); // match
			appendf!(self, "    }}\n"); // fn deserialize
			appendf!(self, "}}\n\n"); // impl PBType
		}
	}
	fn gen_types(&mut self, def: &PunybufDefinition) {
		let mut should_include_hash_map_convertible = false;
		for tp in &def.types {
			if
				tp.get_attrs().contains_key("@builtin") ||
				tp.get_attrs().contains_key("@rust:ignore") ||
				tp.get_attrs().contains_key("@resolve")
			{
				continue;
			}
			if tp.get_attrs().contains_key("@map_convertible") {
				appendf!(self, "impl<K: PBType + std::hash::Hash + Eq, V: PBType> HashMapConvertible<K, V> for {} {{", self.get_type_name(tp));
				should_include_hash_map_convertible = true;
				// TO_MAP contains a leading newline
				appendf!(self, "{}", TO_MAP);
				appendf!(self, "}}\n"); // impl
			}
			match tp {
				PBTypeDef::Alias { alias, doc, .. } => {
					self.gen_doc(doc, 0);
					appendf!(self, "pub type {} = {};\n", self.get_type_name(tp), self.gen_reference(alias, false));
					// impls for aliases are generated automatically
					continue;
				}
				PBTypeDef::Struct { fields, doc, .. } => {
					self.gen_doc(doc, 0);
					appendf!(self, "#[derive(Debug)]\n");
					appendf!(self, "pub struct {} {{\n", self.get_type_name(tp));
					self.gen_fields(fields);
					appendf!(self, "}}\n");
				}
				PBTypeDef::Enum { variants, doc, .. } => {
					self.gen_doc(doc, 0);
					appendf!(self, "#[derive(Debug)]\n");
					appendf!(self, "pub enum {} {{\n", self.get_type_name(tp));
					self.gen_variants(variants);
					appendf!(self, "}}\n");
				}
			}
			appendf!(self, "impl{} PBType for {} {{\n", self.get_type_impl_generics(tp), self.get_type_name(tp));
			appendf!(self, "    const MIN_SIZE: usize = 0; // TODO\n");
			if !tp.get_attrs().is_empty() {
				appendf!(self, "    fn attributes() -> &'static [(&'static str, Option<&'static str>)] {{ &[\n");
				for (name, value) in tp.get_attrs() {
					appendf!(self, "        ({name:?}, {value:?}),\n");
				}
				appendf!(self, "    ] }}\n"); // fn attributes
			}
			appendf!(self, "    {} serialize<W: {}>(&self, w: &mut W) -> io::Result<()> {{\n", self.get_fn(), self.write());
			match tp {
				PBTypeDef::Struct { fields, attrs, .. } => {
					self.gen_serialize_fields(fields, !attrs.contains_key("@sealed"));
					appendf!(self, "        Ok(())\n");
				}
				PBTypeDef::Enum { variants, .. } => {
					appendf!(self, "        match self {{\n");
					self.gen_serialize_variants(variants);
					appendf!(self, "        }}\n");
					appendf!(self, "        Ok(())\n");
				}
				_ => unreachable!()
			}
			appendf!(self, "    }}\n"); // fn serialize
			appendf!(self, "    {} deserialize<R: {}>(r: &mut R) -> io::Result<Self> {{\n", self.get_fn(), self.read());
			match tp {
				PBTypeDef::Struct { fields, attrs, .. } => {
					self.gen_deserialize_fields(fields, !attrs.contains_key("@sealed"));
				}
				PBTypeDef::Enum { variants, .. } => {
					appendf!(self, "        let discriminant = u8::deserialize(r){}?;\n", self.maybe_await());
					appendf!(self, "        Ok(match discriminant {{\n",);
					self.gen_deserialize_variants(variants);
					appendf!(self, "        }})\n");
				}
				_ => unreachable!()
			}
			appendf!(self, "    }}\n"); // fn deserialize
			appendf!(self, "}}\n\n"); // impl PBType
		}
		if should_include_hash_map_convertible {
			// HACK: Because of Rust's orphan rules, we can't put this in the punybuf_common crate.
			appendf!(self, "{}", HASH_MAP_CONVERTIBLE);
			appendf!(self, "\n\n");
		}
	}
	pub fn codegen(mut self, def: &PunybufDefinition) -> String {
		appendf!(self, "#![allow(nonstandard_style)]\n");
		appendf!(self, "///! This file was automatically generated by Punybuf.\n");
		appendf!(self, "///! It's best you don't change anything.\n\n");
		appendf!(self, "use std::io;\n");

		if self.use_tokio {
			appendf!(self, "// if you get an error: tokio's \"io\" feature must be enabled.\n");
			appendf!(self, "use tokio::io::{{AsyncReadExt, AsyncWriteExt}};\n");
		}

		self.uses_common = def.includes_common;

		if def.includes_common {
			if self.use_tokio {
				appendf!(self, "// if you get an error: punybuf_common's \"tokio\" feature must be enabled.\n");
			}
			appendf!(self, "use punybuf_common{}::*;\n", if	self.use_tokio { "::tokio" } else { "" })
		}

		appendf!(self, "\n");

		if !def.commands.is_empty() {
			self.gen_command_enums(def);
		}

		if !def.commands.is_empty() {
			self.gen_commands(def);
		}

		if !def.types.is_empty() {
			self.gen_types(def);
		}

		self.buffer
	}
}