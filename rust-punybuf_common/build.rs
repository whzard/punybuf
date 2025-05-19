use std::env::var;

const PUNYBUF_MAX_BYTES_LENGTH: &str = "PUNYBUF_MAX_BYTES_LENGTH";
const PUNYBUF_MAX_ARRAY_LENGTH: &str = "PUNYBUF_MAX_ARRAY_LENGTH";

fn main() {
	println!(
		"cargo::rustc-env={PUNYBUF_MAX_BYTES_LENGTH}={}",
		var(PUNYBUF_MAX_BYTES_LENGTH).unwrap_or("4294967296".to_string())
	);
	println!(
		"cargo::rustc-env={PUNYBUF_MAX_ARRAY_LENGTH}={}",
		var(PUNYBUF_MAX_ARRAY_LENGTH).unwrap_or("1000000".to_string())
	);
}