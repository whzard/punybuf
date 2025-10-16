# Punybuf
A binary format for encoding strongly-typed data (and RPC).

> Please note, this is not finished yet, expect bugs.

## What is this?
This is the compiler for the PunyBuf Definition language (.pbd). It takes a .pbd file, desugars it, validates it, and outputs a JSON representation that may later be used by other programs for code generation. It also supports natively generating Rust code. (TODO: no it doesn't)

Read about the **[Definition Language and general concepts](docs/Language.md)**

Read about the **[JSON Intermediate Representation](docs/Codegen.md)** (for writing codegen)

Read about the **[Binary format](docs/BinaryFormat.md)** (for writing codegen & [RPC transports](docs/BinaryFormat.md#rpc))

## How do I use this?
```sh
$ pbd ./path/to/file.pbd
```
This will spit out the JSON intermediate representation to `stdout` or put an error to `stderr`. You may then pipe (`|`) this to another program or to a file:
```sh
$ pbd ./path/to/file.pbd | pb2somelanguage -o ./out.somelanguage
$ pbd ./path/to/file.pbd > out.json
```
> "Why use JSON instead of Punybuf?"
> 
> Chicken and egg problem. Not everyone wants to write their project in Rust, and so to write a Punybuf codegen you'd need to be able to use a Punybuf codegen, which is impossible.

Since Rust is supported natively, to generate Rust code, you may use:
```sh
$ pbd ./path/to/file.pbd -o ./out.rs
```
This command won't spit in your `stdout`.

**Usage:**
```
Usage: pbd [OPTIONS] <INPUT>

Arguments:
  <INPUT>  The .pbd definition file

Options:
  -q, --quiet          Do not print JSON into stdout
  -l, --loud           Do print JSON into stdout, overrides -q
  -o, --out <OUT>      Output - only .rs, .json files supported. Implies -q. Allows multiple occurrences.
  -c, --compat <JSON>  Check binary compatibility with the previous version (json file). Aborts if they are not compatible.
  -d, --dry-run        Do not write anything to the filesystem.
      --verbose        Be verbose. Will print a lot of unnecessary things.
      --no-resolve     Skip `@resolve`-ing aliases.
      --no-docs        Do not generate doc-comments. Doesn't affect json.
      --rust:tokio     Generate async rust code for tokio. Affects only `.rs` files from --out.
  -h, --help           Print help
  -V, --version        Print version
```

## Repository structure
- `/pbd` - CLI tool  
- `/docs` - Documentation  
- `/vscode-sytax-highlighting` - VSCode extension for syntax highlighting  
- `/rust_punybuf_common` - Rust crate for `common`  

## TODO
- Add tests
- Add native support for more languages
- Catch self-referential types during validation
- Implement binary compatibility checks
- Document capabilities
- Finish documentation
- Decide what to do with [extension flags](./docs/BinaryFormat.md#extending-structs) (bottom part of this section).
- Support `doc`s inside files
- Rust codegen: restructure to (optionally?) use references instead of owned values
- Rust codegen: use `u64` instead of `UInt`