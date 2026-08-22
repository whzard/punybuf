Here is where the `pbd` source code for all compilation target (codegen) tests lives.

Each compilation target then spits output into its own directory, (which are .gitignored) and does whatever it needs to to test the correctness of its generated code.

Testing this is generally expensive. For example, the Rust compilation target (tests/rust.rs) creates a Cargo crate per test, generates its own tests inside it and runs `cargo check` and `cargo test` on each one. This means both the CPU and disk usage are going to spike quite a bit when running the tests.

This is why harnesses accept a `SKIP_CODEGEN=1` environment variable to automatically skip all codegen tests and not waste your resources.