// Custom built harness to check the correctness of the compiled
// rust code from pbd.

use std::{
	cmp, env, fmt::Write, fs::{self, canonicalize}, hash::{DefaultHasher, Hasher, SipHasher}, mem, ops::Index, panic::{self, catch_unwind}, path::Path, process::Command, sync::mpsc::sync_channel, thread::{self, Builder}, time::SystemTime
};
use json::JsonValue;
use punybuf::{PunybufParser, RustCodegen, ConvertJSON};

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
enum TestResult {
	Pass,
	Warning(String),
	Fail(String),
	#[allow(unused)]
	Skipped
}

fn main() -> Result<(), ()> {
	let mut skip = false;
	let var_skip = env::var("SKIP_CODEGEN").unwrap_or("0".into());
	if var_skip == "1" {
		skip = true;
	}

	let input_test_files = fs::read_dir("tests_codegen/pbd")
		.expect("failed to read directory \"tests_codegen/pbd\"");

	let mut skipped: Vec<String> = vec![];

	let mut inputs = vec![];
	for file in input_test_files {
		let file = file.expect("failed to get directory entry");
			if let Some(ex) = file.path().extension() {
				if ex == "skip" || (skip && ex == "pbd") {
					skipped.push(
						file.path().file_prefix().unwrap()
							.to_string_lossy().into_owned()
					);
					continue;
				}
				if ex != "pbd" {
					continue;
				}
			} else {
				continue;
			}

		inputs.push(file.path());
	}

	let parallelism: usize = thread::available_parallelism()
		.unwrap_or(8.try_into().unwrap()).into();
	// let parallelism = 1;

	let (sender, receiver) = sync_channel::<(String, TestResult)>(16);

	let mut chunk_size = inputs.len() / parallelism;
	if chunk_size == 0 {
		chunk_size = 1;
	}
	for chunk in inputs.chunks(chunk_size) {
		let sender = sender.clone();
		let chunk = chunk.to_owned();
		Builder::new().name(format!("test.{:?}", chunk)).spawn(move || {
			for input in chunk {
				let name = input.file_prefix()
					.expect("invalid test file passed")
					.to_string_lossy().into_owned();

				panic::set_hook(Box::new(|_| {
					// don't print anything out
				}));
				let test_result = catch_unwind(|| {
					run_test(&name, input)
				});
				let test_result = match test_result {
					Err(panicked) => {
						if let Some(string) = panicked.downcast_ref::<String>() {
							TestResult::Fail(string.clone())
						} else if let Some(str) = panicked.downcast_ref::<&'static str>() {
							TestResult::Fail(str.to_string())
						} else {
							TestResult::Fail(format!("{panicked:?}"))
						}
					},
					Ok(test_result) => match test_result {
						None => TestResult::Pass,
						Some(warning) => TestResult::Warning(warning)
					},
				};
				let test_result_string = match test_result {
					TestResult::Pass => "pass",
					TestResult::Warning(_) => "pass + warn",
					TestResult::Fail(_) => "fail",
					TestResult::Skipped => "skip",
				};
				if let Err(..) = sender.send((name.clone(), test_result)) {
					// The listener closed, we don't need it anymore
					eprintln!("failed to send back test result for {name}: {test_result_string}");
					return;
				}
			}
		}).expect("failed to launch thread");
	}

	// Drop the sender so that the iterator can finish
	drop(sender);

	let mut pass_count = 0;
	let mut fail_count = 0;
	let mut skip_count = skipped.len();

	for test_name in skipped {
		println!("{test_name} - {GRAY}skipped{NORMAL}");
	}

	// TODO: collect results and report later for better UX
	for (test_name, result) in receiver.iter() {
		println!("{test_name} - {}", match &result {
			TestResult::Pass => format!("{BOLD}{GREEN}pass{NORMAL}"),
			TestResult::Fail(error) => {
				let error = error.lines()
					.map(|line| format!("\t{}\n", line))
					.fold(String::new(), |mut buf, s| {
						buf.push_str(&s);
						buf
					});
				format!("{BOLD}{RED}fail:{NORMAL}\n{error}\n--- end of output ---\n")
			},
			TestResult::Skipped => format!("{GRAY}skipped{NORMAL}"),
			TestResult::Warning(warning) => format!("{BOLD}{YELLOW}warning:{NORMAL}\n{warning}\n")
		});
		match result {
			TestResult::Pass | TestResult::Warning(_) => pass_count += 1,
			TestResult::Fail(_) => fail_count += 1, // TODO?: stop the output as soon as we fail
			TestResult::Skipped => skip_count += 1,
		}
	}
	println!();
	if fail_count > 0 {
		println!("{BOLD}test result: {RED}fail{NORMAL}.");
	} else {
		println!("{BOLD}test result: {GREEN}ok{NORMAL}.");
	}
	println!(
		"   {} total, \
		{}{pass_count} passed{NORMAL}, \
		{}{fail_count} failed{NORMAL}, \
		{GRAY}{skip_count} skipped{NORMAL} \
		\n",
		pass_count + fail_count + skip_count,
		if pass_count > 0 { GREEN } else { GRAY },
		if fail_count > 0 { RED } else { GRAY },
	);
	if fail_count > 0 {
		Err(())
	} else {
		Ok(())
	}
}

const GEN_ASYNC: bool = true;

fn run_test(name: &str, file: impl AsRef<Path>) -> Option<String> {
	let parsed = PunybufParser::parse_file(&file)
		.expect("failed to read the input file")
		.expect("parse error in the input file");
	if !parsed.includes_common() {
		panic!("files that don't include common are not supported by the harness");
	}
	// TODO: don't hardcode this
	let def = parsed.resolve(true).expect("failed to resolve & validate the input");
	fs::create_dir_all(format!("tests_codegen/rust/{name}/src"))
		.expect("failed to create dir /tests_codegen/rust/{name-of-pbd}/src");

	
	fs::write(format!("tests_codegen/rust/{name}/Cargo.toml"), format!(r#"
# Hi! This file was auto-generated by the Rust test harness
[package]
name = "{name}"
version = "0.1.0"
edition = "2024"

[dependencies]
tokio = {{ version = "1.45.0", features = ["io-util", "rt", "macros", "sync"] }}
punybuf_common = {{ path = "../../../../rust-punybuf_common" }}

[profile.dev]
debug = false
	"#)).expect("failed to write to Cargo.toml");
	
	// if is_dirty(name) {
	// We assume all files are dirty since producing tests
	// is relatively cheap and this breaks test cases when
	// the generated JSON is modified, but the test files are not
	// TODO: Don't parse JSON, instead use the public API of PunybufDefinition
	let converted = def.to_json();
	let value = json::parse(&converted).expect("invalid json produced lol");

	ensure_tests(name, value);
	// }

	let code = RustCodegen::new(GEN_ASYNC, true, &def).codegen();
	fs::write(format!("tests_codegen/rust/{name}/src/proto.rs"), code)
		.expect("failed to write proto.rs");

	// NB: we're using a shared target directory, because
	// otherwise our disk space usage quickly spins out of
	// control

	let cargo_check = Command::new("cargo")
		.current_dir(
			canonicalize(format!("tests_codegen/rust/{name}/"))
				.expect("failed to set environment for cargo")
		)
		.arg("--config")
		.arg("build.target-dir=\"../_shared_target/\"")
		.arg("--config")
		.arg("build.warnings=\"allow\"")
		.arg("check")
		.output()
		.expect("failed to spawn cargo");

	if !cargo_check.status.success() {
		let string = String::from_utf8_lossy(&cargo_check.stderr);
		panic!("cargo check failed: {}", string)
	}

	let cargo_test = Command::new("cargo")
		.current_dir(
			canonicalize(format!("tests_codegen/rust/{name}/"))
				.expect("failed to set environment for cargo")
		)
		.arg("--config")
		.arg("build.target-dir=\"../_shared_target\"")
		.arg("--config")
		.arg("build.warnings=\"allow\"")
		.arg("test")
		.output()
		.expect("failed to spawn cargo");

	if !cargo_test.status.success() {
		let err = String::from_utf8_lossy(&cargo_test.stderr);
		let out = String::from_utf8_lossy(&cargo_test.stdout);
		panic!("cargo test failed: \n===== stderr =====\n{}\n===== stdout =====\n{}", err, out)
	}

	None
}

// #[allow(unused)]
// fn is_dirty(name: &str) -> bool {
// 	// return true;
// 	let Ok(main) = fs::read_to_string(format!("tests_codegen/rust/{name}/src/main.rs")) else {
// 		return true;
// 	};
// 	if !main.starts_with("// date = ") {
// 		return true;
// 	}
// 	let mut lines = main.lines();
// 	let dateline = lines.next().unwrap();
// 	let date = &dateline[("// date = ".len())..];
// 	let Ok(rust_date): Result<u64, _> = date.parse() else {
// 		return true;
// 	};
// 	let Ok(meta) = fs::metadata(format!("tests_codegen/pbd/{name}.pbd")) else {
// 		return true;
// 	};
// 	let Ok(pbd_date) = meta.modified() else {
// 		return true;
// 	};
// 	let paramline = lines.next().expect("expected at least 2 lines");
// 	if paramline != format!("// {};", if GEN_ASYNC { "async" } else { "sync" }) {
// 		return true;
// 	}
// 	let pbd_date = pbd_date.duration_since(SystemTime::UNIX_EPOCH).unwrap().as_secs();
// 	pbd_date > rust_date
// }

fn get_rust_name_and_generics(v: &JsonValue) -> String {
	let mut name = get_rust_name(v);
	let generics = v.index("generic_params");
	let mut is_empty = true;
	for _ in generics.members() {
		if is_empty {
			is_empty = false;
			name.push_str("::<");
		} else {
			name.push_str(", ");
		}
		name.push_str("Cow<'_, str>");
	}
	if !is_empty {
		name.push_str(">");
	}
	name
}

fn get_rust_name(v: &JsonValue) -> String {
	let name = v.index("name");
	let name = name.as_str().expect("invalid json");
	let layer = v.index("layer").as_u32().expect("invalid json");
	let is_highest_layer = v.index("is_highest_layer").as_bool().unwrap_or(false);
	get_rust_ref_name(name, layer, is_highest_layer)
}

fn get_rust_ref_name(name: &str, layer: u32, is_highest_layer: bool) -> String {
	if name == "Optional" {
		return "Option".to_string();
	};
	if is_highest_layer {
		name.into()
	} else {
		format!("{name}_Layer{layer}")
	}
}

fn should_generate_tests(name: &str) -> bool {
	match name {
		"Done" | "Boolean" | "Optional" |
		"Map" | "KeyPair" => false,
		_ => true
	}
}

/// This isn't bulletproof, I'm sure there will be a test scenario
/// where it completely fails because of some random Rust requirement
/// (god please no lifetimes), but oh well
fn ensure_tests(name: &str, definition: JsonValue) {
	let mut main = String::new();
	let date = SystemTime::now();
	let date = date.duration_since(SystemTime::UNIX_EPOCH).unwrap().as_secs();
	writeln!(main, "// date = {date}").unwrap();
	writeln!(main, "// {};", if GEN_ASYNC { "async" } else { "sync" }).unwrap();
	writeln!(main, "// This file was automatically generated by the Rust test").unwrap();
	writeln!(main, "// harness. Refer to /pbd/tests/rust.rs for this.").unwrap();
	writeln!(main).unwrap();
	writeln!(main, "mod proto;").unwrap();
	writeln!(main).unwrap();
	writeln!(main, "fn main() {{").unwrap();
	writeln!(main, r#"  println!("hi! i don't do anything, run me with cargo test!");"#).unwrap();
	writeln!(main, "}}").unwrap(); // fn main
	writeln!(main).unwrap();
	writeln!(main, "#[cfg(test)]").unwrap();
	writeln!(main, "mod test {{").unwrap();
	writeln!(main, "    use crate::proto::*;").unwrap();
	writeln!(main, "    use punybuf_common::*;").unwrap();
	writeln!(main, "    use ::tokio as tokio;").unwrap();
	writeln!(main).unwrap();
	let mut alternator = RustAlternator::new(&definition);
	let types = definition.index("types");
	for ty in types.members() {
		if ty.index("is").as_str().unwrap_or_default() == "alias" ||
			ty.index("attrs").has_key("@builtin") ||
			!should_generate_tests(ty.index("name")
				.as_str().unwrap_or_default())
		{
			continue;
		}

		// TODO: check all possible enum variants instead of
		// basically randomly picking one

		// Streaming:
		writeln!(main, "    #[test]").unwrap();
		writeln!(main, "    fn streaming_{}() {{", get_rust_name(ty)).unwrap();
		writeln!(main, "        let mut vec = Vec::<u8>::new();").unwrap();
		write!(main, "        let thing = ").unwrap();
		alternator.write_type_from_decl(&mut main, ty);
		writeln!(main, ";").unwrap();
		writeln!(main, "        thing.serialize(&mut vec).unwrap();").unwrap();
		writeln!(main, "        let r = &mut &vec[..];").unwrap();
		writeln!(
			main,
			"        let other = {}::deserialize_stream(r).unwrap();",
			get_rust_name_and_generics(ty)
		).unwrap();
		writeln!(main, r#"        assert_eq!(thing, other, "values don't match");"#).unwrap();
		writeln!(main, r#"        assert_eq!(*r, &[], "cleanup");"#).unwrap();
		writeln!(main, "    }}").unwrap(); // streaming_{}

		// Buffer:
		writeln!(main, "    #[test]").unwrap();
		writeln!(main, "    fn in_place_{}() {{", get_rust_name(ty)).unwrap();
		writeln!(main, "        let mut vec = Vec::<u8>::new();").unwrap();
		write!(main, "        let thing = ").unwrap();
		alternator.write_type_from_decl(&mut main, ty);
		writeln!(main, ";").unwrap();
		writeln!(main, "        thing.serialize(&mut vec).unwrap();").unwrap();
		writeln!(main, "        let r = &mut &vec[..];").unwrap();
		writeln!(main,
			"        let other = {}::deserialize(r).unwrap();",
			get_rust_name_and_generics(ty)
		).unwrap();
		writeln!(main, r#"        assert_eq!(thing, other, "values don't match");"#).unwrap();
		writeln!(main, r#"        assert_eq!(*r, &[], "cleanup");"#).unwrap();
		writeln!(main, "    }}").unwrap(); // in_place_{}

		// Async:
		if GEN_ASYNC {
			writeln!(main, "    #[tokio::test]").unwrap();
			writeln!(main, "    async fn async_{}() {{", get_rust_name(ty)).unwrap();
			writeln!(main, "        use punybuf_common::tokio::*;").unwrap();
			writeln!(main, "        let mut vec = Vec::<u8>::new();").unwrap();
			write!(main, "        let thing = ").unwrap();
			alternator.write_type_from_decl(&mut main, ty);
			writeln!(main, ";").unwrap();
			writeln!(main, "        thing.serialize_async(&mut vec).await.unwrap();").unwrap();
			writeln!(main, "        let r = &mut &vec[..];").unwrap();
			writeln!(
				main,
				"        let other = {}::deserialize_async(r).await.unwrap();",
				get_rust_name_and_generics(ty)
			).unwrap();
			writeln!(main, r#"        assert_eq!(thing, other, "values don't match");"#).unwrap();
			writeln!(main, r#"        assert_eq!(*r, &[], "cleanup");"#).unwrap();
			writeln!(main, "    }}").unwrap(); // async_{}
		}
	}
	// TODO: generate tests for commands
	writeln!(main, "}}").unwrap(); // mod test

	fs::write(format!("tests_codegen/rust/{name}/src/main.rs"), main)
		.expect("failed to write main.rs");
}

const TEST_NUMS: &[u8] = &[
	0, 32, 64, 127
];
const TEST_FLOATS: &[f32] = &[
	0., 1., 123., -225., 0.6767,
	3.14159265, -0.98112,
];

const TEST_UINTS: &[u64] = &[
	0, 32, 64, 127, 128, 129,
	16511, 16512, 16513,
	2113663, 2113664, 2113665,
	68721590399, 68721590400, 68721590401,
	1152921573328437375
];

const TEST_STRINGS: &[&str] = &[
	"Hello, world!", "Witaj, świecie",
	"Привіт, світ", "你好世界",
	"ಹಲೋ ವರ್ಲ್ಡ್", "emoji \u{2764} \u{1F63A}"
];

/// Used to keep test results deterministic per file.
/// Will alternate between different values for strings, numbers, bytes
/// and so on
struct RustAlternator<'a> {
	definition: &'a JsonValue,
	generic_args: Option<JsonValue>,
	generic_params: Vec<String>,
	/// This is needed in order to have deep nested structures
	/// without crashing the process.
	depth: usize,
	// The deprecated SipHasher is used here because it allows us
	// to output deterministic but random-looking output
	#[allow(deprecated)]
	hasher: SipHasher
}

#[allow(deprecated)]
impl<'a> RustAlternator<'a> {
	fn new(definition: &'a JsonValue) -> Self {
		Self {
			definition,
			generic_args: None,
			generic_params: Vec::new(),
			depth: 0,
			hasher: SipHasher::new_with_keys(24, 42),
		}
	}
	fn write_type_from_decl(&mut self, w: &mut String, ty: &JsonValue) {
		if ty.index("is").as_str().unwrap_or_default() != "alias" {
			write!(w, "{}", get_rust_name(ty)).unwrap();
		}
		let mut saved_gen_params = Vec::new();
		for param in ty.index("generic_params").members() {
			saved_gen_params.push(param.as_str().expect("invalid json").to_owned());
		}
		mem::swap(&mut saved_gen_params, &mut self.generic_params);
		match ty.index("is").as_str().unwrap_or_default() {
			"struct" => {
				self.write_struct(w, ty);
			}
			"enum" => {
				self.write_enum(w, ty);
			}
			"alias" => {
				let rf = ty.index("alias");
				self.write_ref(w, rf, true);
			}
			_ => {
				panic!("invalid json");
			}
		}
		self.generic_params = saved_gen_params;
	}
	fn write_enum(&mut self, mut w: &mut String, ty: &JsonValue) {
		let variants = ty.index("variants");
		let num_vars = variants.len();
		self.hasher.write(b"enum");
		let n = self.hasher.finish() as usize;
		// eprintln!("{num_vars} {ty}");
		let mut selected_variant = variants.index(n % num_vars);
		if self.depth >= 5 {
			let mut found = false;
			// Try to select a variant that doesn't go deeper
			for candidate in variants.members() {
				if candidate.index("value").is_null() {
					selected_variant = candidate;
					found = true;
					break;
				}
			}
			if !found {
				// Try one that isn't circular (4th index is `is_circular`)
				for candidate in variants.members() {
					let value = candidate.index("value");
					if !value.index(4).as_bool()
							.is_some_and(|b| b) {
						selected_variant = candidate;
						found = true;
						break;
					}
				}
			}
			if !found {
				eprintln!(
					"{YELLOW}warning in test generation: \
					failed to select a variant of `{}` \
					that doesn't recurse.{NORMAL}",
					ty.index("name")
				)
			}
		}
		let value = selected_variant.index("value");
		if value.is_null() {
			// Rust needs to know the type
			let generics = ty.index("generic_params");
			let mut is_empty = true;
			for _ in generics.members() {
				if is_empty {
					is_empty = false;
					write!(&mut w, "::<").unwrap();
				} else {
					write!(&mut w, ", ").unwrap();
				}
				write!(&mut w, "Cow<'_, str>").unwrap();
			}
			if !is_empty {
				write!(&mut w, ">").unwrap();
			}
		}
		write!(
			&mut w, "::{}",
			selected_variant.index("name").as_str().expect("variant name")
		).unwrap();
		if !value.is_null() {
			write!(&mut w, "(").unwrap();
			self.depth += 1;
			self.write_ref(w, value, true);
			self.depth -= 1;
			write!(&mut w, ")").unwrap();
		}
	}
	fn write_struct(&mut self, mut w: &mut String, ty: &JsonValue) {
		let fields = ty.index("fields");
		write!(w, " {{ ").unwrap();
		for field in fields.members() {
			let flags = field.index("flags");
			if flags.is_array() {
				let mut flag_set = true;
				let mut flag_val_set = true;
				for flag in flags.members() {
					write!(w, "{}: ", flag.index("name").as_str().unwrap()).unwrap();
					if flag.index("value").is_null() {
						write!(&mut w, "{}, ", flag_set).unwrap();
						flag_set = !flag_set;
					} else {
						if flag_val_set && self.depth >= 5 {
							write!(&mut w, "Some(").unwrap();
							self.depth += 1;
							self.write_ref(&mut w, flag.index("value"), true);
							self.depth -= 1;
							write!(&mut w, "), ").unwrap();
						} else {
							// Theoretically, rust might be unable to
							// figure the type of Option
							write!(&mut w, "None, ").unwrap();
						}
						flag_val_set = !flag_val_set;
					}
				}
				continue;
			}
			write!(w, "{}: ", field.index("name").as_str().unwrap()).unwrap();
			self.write_ref(&mut w, field.index("value"), true);
			write!(w, ", ").unwrap();
		}
		write!(w, "}}").unwrap();
	}
	fn get_string(&mut self) -> &'static str {
		self.hasher.write(b"string");
		let n = self.hasher.finish() as usize;
		let string = TEST_STRINGS[n % TEST_STRINGS.len()];
		string
	}
	fn get_integer(&mut self) -> u8 {
		self.hasher.write(b"integer");
		let n = self.hasher.finish() as usize;
		let number = TEST_NUMS[n % TEST_NUMS.len()];
		number
	}
	fn get_float(&mut self) -> f32 {
		self.hasher.write(b"float");
		let n = self.hasher.finish() as usize;
		let float = TEST_FLOATS[n % TEST_FLOATS.len()];
		float
	}
	fn get_uint(&mut self) -> u64 {
		self.hasher.write(b"uint");
		let n = self.hasher.finish() as usize;
		let uint = TEST_UINTS[n % TEST_UINTS.len()];
		uint
	}
	fn write_builtin_ref(
		&mut self, mut w: &mut String,
		name: &str, layer: u32, is_highest_layer: bool
	) -> bool {
		// eprintln!("builtin {name}");
		if layer != 0 || !is_highest_layer {
			return false;
		}
		match name {
			"String" => {
				write!(&mut w, r#"Cow::from("{}")"#, self.get_string()).unwrap();
				true
			}
			"UInt" => {
				write!(&mut w, r#"UInt({})"#, self.get_uint()).unwrap();
				true
			}
			"Bytes" => {
				write!(&mut w, r#""{}".as_bytes().to_owned().into()"#, self.get_string()).unwrap();
				true
			}
			"U64" | "U32" | "U16" | "U8" |
			// TODO: have a separate category for possibly
			// negative values
			"I32" | "I64" =>
			{
				write!(&mut w, r#"{}"#, self.get_integer()).unwrap();
				true
			}
			"F32" | "F64" => {
				let gotten = self.get_float();
				if gotten.fract() == 0. {
					write!(&mut w, r#"{}.0"#, gotten).unwrap();
				} else {
					write!(&mut w, r#"{}"#, gotten).unwrap();
				}
				true
			}
			"Array" => {
				if self.depth >= 5 {
					write!(&mut w, r#"vec![]"#).unwrap();
					return true;
				}
				write!(&mut w, r#"vec!["#).unwrap();
				let generic_args = self.generic_args.clone().unwrap();
				for _ in 0..3 {
					self.depth += 1;
					self.write_ref(&mut w, &generic_args.index(0), false);
					self.depth -= 1;
					write!(&mut w, ", ").unwrap();
				}
				write!(&mut w, r#"]"#).unwrap();
				true
			}
			"Map" => {
				if self.depth >= 5 {
					write!(&mut w, r#"vec![]"#).unwrap();
					return true;
				}
				write!(&mut w, r#"vec!["#).unwrap();
				let generic_args = self.generic_args.clone().unwrap();
				for _ in 0..3 {
					write!(&mut w, r#"KeyPair {{ key: "#).unwrap();
					self.depth += 1;
					self.write_ref(&mut w, &generic_args.index(0), false);
					write!(&mut w, ", value: ").unwrap();
					self.write_ref(&mut w, &generic_args.index(1), false);
					self.depth -= 1;
					write!(&mut w, "}}, ").unwrap();
				}
				write!(&mut w, r#"]"#).unwrap();
				true
			}
			"Boolean" => {
				self.hasher.write(b"boolean");
				let n = self.hasher.finish() as usize;
				if n % 2 == 0 {
					write!(&mut w, r#"true"#).unwrap();

				} else {
					write!(&mut w, r#"false"#).unwrap();					
				}
				true
			}
			"Done" => {
				write!(&mut w, r#"Done {{}}"#).unwrap();
				true
			}
			_ => false
		}
	}
	fn resolve_generic_args(&self, gens: &JsonValue) -> JsonValue {
		let mut result = JsonValue::Array(vec![]);
		for g in gens.members() {
			let name = g.index(0).as_str().expect("invalid Ref");
			let generic_args = g.index(2);

			let generic_args = self.resolve_generic_args(generic_args);

			if let Some((index, ..)) = self.generic_params.iter()
				.enumerate()
				.find(|(_, p)| *p == name)
			{
				let mut reference = self.generic_args.as_ref().map(|args| args.index(index));
				if let Some(x) = reference && x.is_null() {
					reference = None;
				}
				match reference {
					Some(rf) => {
						result.push(rf.clone()).unwrap();
						continue;
					}
					None => {}
				}
			}
			let mut clone = g.clone();
			// Insert our resolved generic args back
			clone[2] = generic_args;
			result.push(clone).unwrap();
		}
		result
	}
	fn write_ref(&mut self, mut w: &mut String, rf: &JsonValue, do_box: bool) {
		// eprintln!("rf {rf}");
		let name = rf.index(0).as_str().expect("invalid Ref");
		let layer = rf.index(1).as_u32().unwrap_or(0);
		let generic_args = rf.index(2);
		let is_highest_layer = rf.index(3).as_bool().unwrap_or(true);
		let is_circular = rf.index(4).as_bool().unwrap_or(false);

		let mut saved_generic_args = Some(self.resolve_generic_args(generic_args));
		mem::swap(&mut saved_generic_args, &mut self.generic_args);

		// eprintln!("parsing gen args");
		if let Some((index, ..)) = self.generic_params.iter()
			.enumerate()
			.find(|(_, p)| *p == name)
		{
			let mut reference = saved_generic_args.as_ref().map(|args| args.index(index));
			if let Some(x) = reference && x.is_null() {
				reference = None;
			}
			match reference {
				Some(rf) => {
					// eprintln!("got ref at {index}: {rf}");
					self.write_ref(&mut w, rf, true);
					mem::swap(&mut saved_generic_args, &mut self.generic_args);
					return;
				}
				None => {
					// For now, replace unknown generics with strings
					write!(&mut w, r#"Cow::from("{}")"#, self.get_string()).unwrap();
					self.generic_args = saved_generic_args;
					return;
				}
			}
		}
		// eprintln!("/parsing gen args");

		if self.write_builtin_ref(&mut w, name, layer, is_highest_layer) {
			mem::swap(&mut saved_generic_args, &mut self.generic_args);
			return;
		}

		let rs_name = get_rust_ref_name(name, layer, is_highest_layer);
		let typedef = self.definition.index("types")
			.members()
			.find(|ty| get_rust_name(ty) == rs_name)
			.expect("cannot find type");
		if do_box && is_circular {
			write!(&mut w, "std::boxed::Box::new(").unwrap();
		}
		self.write_type_from_decl(w, typedef);
		if do_box && is_circular {
			write!(&mut w, ")").unwrap();
		}
		// FIXME: I'm not sure if this will work, it might be impossible
		// to resolve multiple levels of references
		self.generic_args = saved_generic_args;
	}
}