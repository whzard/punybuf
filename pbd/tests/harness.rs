use std::{env, fs, panic::catch_unwind, path::Path, fmt::Write};
use diff_json::compare_json;
use punybuf::{PunybufParser, convert_full_definition};

pub const RED: &str = "\x1b[91m";
pub const BLUE: &str = "\x1b[94m";
pub const YELLOW: &str = "\x1b[93m";
pub const NORMAL: &str = "\x1b[0m";
pub const GRAY: &str = "\x1b[30m";
pub const GREEN: &str = "\x1b[32m";
#[allow(unused)]
pub const INTENSE: &str = "\x1b[97m";
pub const BOLD: &str = "\x1b[1m";

enum TestResult {
	Pass,
	Warning(String),
	Fail(String),
	NoExpectedResult(String),
	Skipped
}

fn main() -> Result<(), ()> {
	let mut record = false;
	let var_record = env::var("RECORD").unwrap_or("0".into());
	if var_record == "1" {
		record = true;
	}

	let test_files = fs::read_dir("test_files").expect("failed to read directory \"test_files\"");
	let mut results: Vec<(String, TestResult)> = vec![];
	for file in test_files {
		let file = file.expect("failed to get directory entry");
		let _path = file.path();
		let test_name = _path.file_prefix().unwrap().to_str().unwrap();
		if file.path().extension().is_none() || file.path().extension().unwrap() != "pbd" {
			continue;
		}
		let expected_result = fs::read_to_string(
			file.path().parent().unwrap().join(format!("~{}.result", test_name))
		).ok();

		if let Some(ex_result) = &expected_result {
			if ex_result.lines().next().unwrap_or("") == "!skip" {
				results.push((test_name.to_string(), TestResult::Skipped));
				continue;
			}
		}

		let test_result = catch_unwind(|| {
			eprintln!("\nrunning test {}", test_name);
			run_test(file.path(), expected_result)
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
				Ok(None) => TestResult::Pass,
				Ok(Some(warning)) => TestResult::Warning(warning),
				Err(no_case) => TestResult::NoExpectedResult(no_case)
			},
		};
		results.push((test_name.to_string(), test_result));
	}
	eprintln!("\nall tests finished.\n");
	let mut pass_count = 0;
	let mut fail_count = 0;
	let mut skip_count = 0;
	// TODO: improve testing output, when something fails
	// (right now, you just have to compare JSONs - from the command line!)
	for (test_name, result) in results {
		println!("{test_name} - {}", match &result {
			TestResult::Pass => format!("{BOLD}{GREEN}pass{NORMAL}"),
			TestResult::Fail(error) =>
				format!("{BOLD}{RED}fail:{NORMAL}\n{error}\n--- end of output ---\n"),
			TestResult::NoExpectedResult(needed_value) =>
				format!("{BOLD}{RED}no expected result, got:{NORMAL}\n{needed_value}\n"),
			TestResult::Skipped => format!("{GRAY}skipped{NORMAL}"),
			TestResult::Warning(warning) => format!("{BOLD}{YELLOW}warning:{NORMAL}\n{warning}\n")
		});
		match result {
			TestResult::Pass | TestResult::Warning(_) => pass_count += 1,
			TestResult::Fail(_) => fail_count += 1,
			TestResult::Skipped => skip_count += 1,
			TestResult::NoExpectedResult(contents) => {
				fail_count += 1;
				if record {
					// the `~` is so that all the result files are displayed
					// below test files using alphabetical sorting
					let path = format!("./test_files/~{test_name}.result");
					fs::write(&path, contents)
						.expect("writing failed");
					println!("{YELLOW}warning: wrote expected result to {path}{NORMAL}");
				} else {
					println!(
						"{YELLOW}tip: set RECORD=1 to automatically write the expected result{NORMAL}"
					)
				}
			},
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
		{}{pass_count} passed, \
		{}{fail_count} failed, \
		{GRAY}{skip_count} skipped \
		{NORMAL}\n",
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

fn run_test(file: impl AsRef<Path>, expected: Option<String>) -> Result<Option<String>, String> {
	let parse_result = PunybufParser::parse_file(file).expect("failed to read file");
	let parsed = match parse_result {
		Ok(x) => x,
		Err(err) => {
			if let Some(expected) = expected {
				let mut lines = expected.lines();
				let expected_status = lines.next().expect("invalid test result file");
				if expected_status != "!error/parser" {
					panic!(
						"invalid status: expected {expected_status:?}, \
						got \"!error/parser\" with this error:\n\
						{err}"
					);
				}
				let expected_error = lines.next().unwrap_or("<no error>");
				if expected_error != err.error.content {
					return Ok(Some(
						format!(
							"did not match the exact error: got `{}`, expected `{}`",
							err.error.content, expected_error
						)
					));
				}
				return Ok(None);
			}
			return Err(format!(
				"!error/parser\n\
				{}\n\
				# This file was auto-generated by harness.rs",
				err.error.content
			));
		}
	};
	let definiton = match parsed.resolve(true) {
		Ok(x) => x,
		Err(err) => {
			if let Some(expected) = expected {
				let mut lines = expected.lines();
				let expected_status = lines.next().expect("invalid test result file");
				if expected_status != "!error/validator" {
					panic!(
						"invalid status: expected {expected_status:?}, \
						got \"!error/validator\" with this error:\n\
						{err}"
					);
				}
				let expected_error = lines.next().unwrap_or("<no error>");
				if expected_error != err.error.content {
					return Ok(Some(
						format!(
							"did not match the exact error: got `{}`, expected `{}`",
							err.error.content, expected_error
						)
					));
				}
				return Ok(None);
			}
			return Err(format!(
				"!error/validator\n\
				{}\n\
				# This file was auto-generated by harness.rs",
				err.error.content
			));
		}
	};
	let json_result = convert_full_definition(&definiton);
	if let Some(mut expected) = expected {
		let mut lines = expected.lines();
		let expected_first = lines.next().expect("invalid test result file");
		if expected_first != "!success" {
			panic!("got success, expected {}", expected)
		}
		// ignore first line
		let mut expected = expected.split_off(
			expected.find('\n').expect("invalid test result file") + 1
		);
		// ignore last line
		_ = expected.split_off(expected.find('\n').expect("invalid test result file"));
		if let Some(diff) = format_json_diffs(expected.trim(), json_result.trim()) {
			panic!(
				"json does not match:\n\
				{BOLD}{YELLOW}expected:{NORMAL} {}\n\n\
				{BOLD}{RED}     got:{NORMAL} {}\n\n\
				difference:\n{diff}",
				expected.trim(), json_result.trim()
			)
		}
		return Ok(None);
	}
	return Err(format!(
		"!success\n\
		{}\n\
		# This file was auto-generated by harness.rs",
		json_result
	));
}

fn format_json_diffs(old_value: &str, new_value: &str) -> Option<String> {
	let mut result = String::new();

	let diffs = compare_json(old_value.trim(), new_value.trim())
		.expect("failed to diff json");

	if diffs.is_empty() {
		return None;
	}

	for diff in diffs {
		let prefix = match diff.diff_type {
			diff_json::DiffType::Added =>
				format!("{GREEN }+ added:{NORMAL}"),
			diff_json::DiffType::Removed =>
				format!("{RED   }- removed:{NORMAL}"),
			diff_json::DiffType::Modified =>
				format!("{YELLOW}~ modified:{NORMAL}"),
			diff_json::DiffType::Moved =>
				format!("{BLUE  }> moved:{NORMAL}"),
		};
		writeln!(result, "{prefix} {}", diff.path).unwrap();
		if let Some(val) = diff.old_value {
			writeln!(result, "{RED}-  {}{NORMAL}", val).unwrap();
		}
		if let Some(val) = diff.new_value {
			writeln!(result, "{GREEN}+  {}{NORMAL}", val).unwrap();
		}
		result.push('\n');
	}

	// result.push_str(NORMAL);
	return Some(result);
}