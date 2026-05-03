> Note: "implementation" is a program that takes a Punybuf definition and outputs code to handle it in a real programming language.

# Attributes
An attribute can be applied to a type, alias, enum, variant, field, flag, or a command.

Some attributes are applied only during compilation, some attributes are applied during code generation.

Implementations may define their own attributes if they want to support additional features.

## `@resolve`
> applied to **aliases** by the **compiler**

Resolve the alias, replacing all instances of this alias with the aliased type. Works with generics.
```pbd
@resolve
Strings = Array<String>

Type = {
	field: Strings
}
# gets converted to
Type = {
	field: Array<String>
}
```

Disabled by the `--no-resolve` flag.

## `@extension`
> applied to **flags** or **enum variants** by the **implementation**, checked by the compiler

Mark this flag or this variant as an extension. [Extensions](Language.md#extensions) and [how to implement them](BinaryFormat.md#extensions).

Conflicts with [`@sealed`](#sealed) on the parent struct.  
Invalid when no [`@default`](#default) variant exists on the enum.

## `@extension_flags`
> applied to **one flag field** by the **implementation**, checked by the compiler

Mark this field as the extension flag field. Must be a flag field. All other flag fields on the struct must be exhausted. See [Extensions](Language.md#extensions).

Conflicts with [`@sealed`](#sealed) on the parent struct.  

## `@capability(name)`
> applied to **commands**, is informative, but may be checked by the RPC implementation

Mark this command only available when a capability with the name `name` is available. The way the available capabilities are made known is RPC-implementaition defined.

## `@sealed`
> applied to **structs** or **commands** by the **implementation**, checked by the compiler

Disallow [extensions](Language.md#extensions) on this struct.

## `@default`
> applied to **enum variants** by the **implementation**, checked by the compiler

Mark this enum variant as the default and allow the enum to be [extensible](Language.md#extending-enums).

## `@name(overridden_name)`
> applied to **commands** by the **compiler**

Override the name of this command during ID generation: The ID will be generated using `crc32("{overridden_name}.{layer}")`, where `layer` is the layer of this command.

You can put anything inside the `overridden_name`, including special characters. This is also useful for renaming the command while still keeping the old ID.

Conflicts with [`@id`](#idoverridden_id) on this command.

## `@id(overridden_id)`
> applied to **commands** by the **compiler**

Override the ID of this command. Careful, the ID must be unique across all layers.

Conflicts with [`@name`](#nameoverridden_name) on this command.

## `@builtin`
> applied to **any type** by both the **compiler** and the **implementation**

Mark this type as built-in, as in provided externally. This is what `common` uses. Any validation is skipped on this type and implementations must ignore built-ins.

## `@void`
> applied to **the special `Void` type only**, by the **compiler**, requires special handling by the **implementation**

Allows to define the `Void` type. `@builtin` implied.

## `@flags(n)`
> applied to **`@builtin` types** by the **compiler**

Allow defining [flag fields](Language.md#flag-fields) using this type. Allows up to `n` flags.

## `@map_convertible`
> applied to **Map-like types** by the **implementation**

Depending on the language, allow conversions of this type to `Map`, `HashMap`, or anything like that.

# Implementation-specific attributes
These attributes are, well, implementation-specific and usually only affect one codegen. If you're writing your own codegen, you may add whatever you want here, provided you prefix it with your implementation's name.

## Rust
### `@rust:ignore`
Ignores the next type or command.

### `@rust:use(name)`
Works only on `@builtin`s. Alias this type to the one specified by `name`, where `name` should be a fully qualified type name. That type must already implement the `PBType` trait.