Some libraries may not provide a native implementation for Punybuf RPC. If that's what you came here for, [skip to here](#rpc).

# Binary format
Provided the implementation knows a type, it must be able to always decode the value without any additional properties (like length). This means implementations must be able to be streamed back-to-back with the `deserialize()` function repeatedly called on the stream.

Any value must have exactly one way of being encoded. If and only if `val1 == val2`, `serialize(val1)` must always be bit-for-bit equal to `serialize(val2)`

## Types
After resolving aliases, there are three kinds of types:

### Built-ins
The `common` definition defines several `@builtin` types. They must be provided by an external library, as opposed to being generated.

#### U8, U16, U32, U64, I32, I64, F32, F64
These numbers are encoded in *big-endian*. Their length is obvious from the name of the type.

#### UInt
A variable-length unsigned integer. The format for this integer is as follows:  

```
0xxxxxxx
10xxxxxx xxxxxxxx + 128
110xxxxx xxxxxxxx xxxxxxxx + 16512
1110xxxx xxxxxxxx xxxxxxxx xxxxxxxx xxxxxxxx + 2113664
1111xxxx xxxxxxxx xxxxxxxx xxxxxxxx xxxxxxxx xxxxxxxx xxxxxxxx xxxxxxxx + 68721590400
```
The first bits (length bits) of the first octet represent the amount of octets needed for the whole number, as defined by the figure above.  
If we stopped there, there would be multiple ways of representing small numbers, e.g. `52` could be both written as `00110100` and `10000000 00110100`. To prevent this and to also pack more numbers per byte, punybuf's varints pack additional information into the length bits: since the largest possible number that we can represent with 1 octet is `01111111 = 127`, the smallest possible number we are able to represent with 2 octets shall be `128`, represented as `10000000 00000000`. Therefore, if a varint takes 2 octets, we must add `128` to it, and so on, and so forth.

Since the greatest number we can represent, 1152921573328437375, doesn't work out to a power of two, for safety and clarity, implementations may decide to set the maximum possible representable number to 2^60.

So, a `UInt` in Protobuf must deserialize to a number that can hold 2^60 bits, usually a 64-bit integer.

> **Rationale:**  
> For performance reasons, we'd like for the entire length of the number to be known as soon as the first byte is read, so Protobuf-style numbers are not possible. However, most numbers are small. Using QUIC-style numbers, where the first two bits encode the length, means that we'd be limited to just 64 numbers we can represent with 1 byte. This seems like an acceptable trade-off, where small numbers (<16512) can be easily represented with 2 bytes, medium numbers (<2113664) can be represented with 3, and the uncommon larger numbers can be represented with either 5 or 8 bytes, because even large numbers, like the entire population, rarely exceed 68 billion (but do exceed 200 million, which we could fit in 4 bytes).

#### Array
The type `Array<T>` is represented in memory as a `UInt`, representing the number of items `n`, immediately followed by `T*n`.

Implementations must limit the maximum number of items, as decribed in the [following section](#bytes).

#### Bytes
In practice, the same as `Array<U8>`, i.e. a `UInt` representing the length followed by that many bytes. Provided as a separate `@builtin` type to allow for optimizations, like allocating the buffer space prior to consumption.

Since the length is represented by a `UInt` and the largest value that can be represented with it is 1152921573328437376, this would theoretically allow for up to a little over ***1024 Pebibytes*** (!) of encoded information. Punybuf values are meant to be small-to-medium sized so they can fit into memory. To prevent crashes due to malicious or malformed values, implementations must set a hard limit of at most **4 Gibibytes (4294967296 bytes)** and are encouraged to set lower limits and to allow the user to choose the limit themselves. This also applies to `String`s and `Array`s (limit the amount of items in the case of the latter).

#### String
The same as `Bytes`, except the contents of this should be valid UTF-8 data. Note that the length of the string is in bytes, not code points. If the contents are not valid UTF-8, they should be lossily converted, i.e. replaced by the unicode replacement character.

Implementations must limit the maximum length, as decribed in the [previous section](#bytes).

#### Map
Represented as an `Array<KeyPair<K, V>>`, where `KeyPair<K, V> = { key: K value: V }`.

This isn't a `@builtin` type and it MUST NOT be immediately converted to the map of your language upon deserialization, unless, by some miracle: (1) the Map in your language preserves the order of elements and (2) the Map in your language allows duplicate keys.

The implementation may provide convenience methods for converting this `Map` into the Map of its language, however it must handle these edge cases with grace (delegating control to its user when needed).

### Structs
A struct holds multiple values of various types. These values are encoded back-to-back with no padding or gaps inbetween.  
To encode something like `Struct = { x: X y: Y }`, the implementation must first encode `X`, then `Y` (and also maybe extensions, more on that later).

#### Flag fields
A flag field is represented by a number, in which each bit corresponds to either a boolean value, or an optional type value, called a flag. Flags must be read in order, starting from the *least significant bit*. If the value of a flag field is a `UInt`, the bits are read from the deserialized 64-bit value (only 60-bit are available in practice, allowing up to 60 flags).

Some flags take optional values, where whether the flag is set or not represents whether the value (flag value) is present or not. The flag value is, if present, placed right after the flag field (except for extensions), after all previous flag values.
```
User = {
	flags: U8.{
		likes_cats?               # simple boolean flag
		preferred_name?: String   # flag with optional value
		has_friends?
		preferred_data_serialization_format?: String
	}
	name: String
}
```
The above struct's flags could have 16 invariants (ignoring possible string values), here are some of them:
```
00000001 {String} <-- name (always present)
       ^
       likes_cats?

------------------------------

      preferred_name set?
      |
      v
00000000 {String}

------------------------------

      preferred_name set?
      |
      v
00000010 {String} {String}
         ^
         |
         preferred_name

------------------------------

      +--+- preferred_name?
      |  |
      v  v
00000110 {String} {String}
     ^
     has_friends?

------------------------------

      +--+- preferred_name?
      |  |
      v  v
00001110 {String} {String} {String}
    ^             ^
    |             |
    +-------------+- preferred_data_serialization_format?
```
> Note that this diagram does not include [extensions](#extensions).

Perhaps a bit counterintuitively, the last (least significant) bit representing a flag value corresponds to the first flag value.

Implementations MUST ignore unknown flags when deserializing and MUST set all unspecified flags to `0` when serializing.

#### Quick note on extensions
Unless otherwise specified by `@sealed`, all structs (including [command arguments](#encoding-commands)) have a single `UInt` at the end that tells you how many more bytes to consume before finishing processing this struct. Extensions are discussed in detail [below](#extending-structs).

### Enums
Enumerations are represented by one octet, representing the enum variant, optionally followed by the value of the associated type of the variant.

> Value-enums are just syntax sugar and your implementation most likely won't even know if a certain enum is a value enum.

```
Mood = [
	Happy,                 # = 0
	Sad,                   # = 1
	LockedTFIn,            # = 2
	ThinkingAbout: String  # = 3, then {String}
]
```

#### Quick note on extensions
If an enum variant is marked by `@default`, this enum supports extensions. If the value is unknown, set the enum value to the default variant. `@default` variants never have an associated type. Extensions are discussed in detail [below](#extending-enums).

## Commands
This section is applicable only for Punybuf RPC (commands). Commands are just types that have special meaning assigned to them. Commands can be "sent" (we use "invoked") from both parties, they can either respond with a return value, return an error, or provide no confirmation (`Void`).

This specification expects a reliable bidirectional stream. These expectations may be fulfilled in part by using the Punybuf RPC (like providing reliable delivery using special commands) if necessary, but they may require some modification to the protocol not described here.

### Encoding commands
A command really has three types: the *Argument* type, the *Return* type, and the *Error* type (always an enum).

Commands are written in the language like this:
```pbd
command: { this_is_a: Struct } -> Return ![SomeError]
command_no_argument: () -> Return ![SomeError]
command_named_argument: SomeArgument -> Return
useless_command: () -> Void
```
A command should usually be a separate struct, class or object in your language, containing the argument, even when its argument is named and even when its argument doesn't exist.

Each command has a unique command ID. This ID is generated as a `crc32_cksum` of the command name, a period (`.`) and its layer number. ([The concept of layers](./Language.md#layers) and [How to handle them](#layers)) This command ID is already provided in the [JSON IR](./Codegen.md).

To serialize a command is to serialize its command ID as a `U32`, followed by the serialization of the argument.

Each command type in your language should have some way of having associated *Return* and *Error* types.

### Error types
Any command can fail (except for those that return `Void`). That means that the error enum is **always present**.

A user-defined error enum starts with `1`, because a `0` variant is always reserved for the "unknown error" case. This `0` variant has an associated type of `String`.

You can think of this as a sort of syntax sugar. This:
```
mayFail: () -> Nil ![SomeError, SomeOtherError]
```
...must get converted to this:
```
mayFail: () -> Nil ![_UnknownError_: String, SomeError, SomeOtherError]
```

### RPC
A Punybuf frame has a header, which is just one 32-bit *big-endian* value:
```
+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+
|R|E|                  command sequence number                  |
+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+
```
where  
`R` - whether this is a response frame  
`E` - whether this is an error

The sequence number uniquely identifies a command invokation. There are two **command sequences** in a given connection, one per peer.

Implementations must store an `out_next_seq` number, initialize it to `1`, and increment it every time after they invoke a command.  
They also must be able to map their sequence numbers to command invocations.

Depending on the frame type, the command sequence number references either the sender', or the receiver's command sequence. 

`R` `E`|type of frame|command sequence
-------|-|-
`0` `0`|`COMMAND`|sender
`0` `1`|`FRAME_REJECTED`|unknown
`1` `0`|`RESPONSE_RETURN`|receiver
`1` `1`|`RESPONSE_ERROR`|receiver

When invoking a command, unless it's supposed to return `Void`, associate the `out_next_seq` number with that command invocation.  
When receiving a valid `RESPONSE_*` frame, look up its `seq` in the association map and parse the body of the frame as either the *Return* or the *Error* type of the command that was invoked with that seq number.  
When receiving a `RESPONSE_*` frame with its `seq` >= `out_next_seq` or not present in the mapping, the frame must be [rejected](#rejection).  

For unreliable transports, incoming command sequence numbers may be used to identify frames on the wire, but discarding frames needs to be done with extra care.  
When receiving `COMMAND` frames, implementations may remember which sequence number they had, and [reject](#rejection) subsequent commands with the same sequence number.

To focus on the happy path,

-|When sending|When receiving
-|-|-
`COMMAND` frames|associate the next seq number with that command invocation, increment the next seq number|perform the command, then send back a `RESPONSE_*` frame
`RESPONSE_*` frames|set the seq number of the frame to the seq of the command you've just performed (or failed)|identify the command this response is responding to, based on the seq number, then handle the result

All data that lies "inside of frames" is located directly after the header and is always possible to parse either because it contains a command ID, or because it identifies a *Return* or *Error* type of an existing invoked command by its `seq` number.

When deserialization of a command or a response fails, that frame must be rejected as described below.

#### Rejection
Rejecting a frame is done whenever deserialization fails and/or the peer is no longer able to parse the stream.

To reject a frame, the implementation must send a `FRAME_REJECTED` frame (`01`) on the stream, with the sequence number equal to the seq number of the rejected frame. Note that it might be ambiguous what frame this references as the sequences are independent of each other. After the header, encode a `String` value, as you normally would if it were a simple error, with a human-readable description of the error, like `"parsing the command failed"` or `"unknown command referenced"`, ideally so it becomes clear which frame you're referring to.

After a rejection, if the implementation can't read any more bytes from the incoming stream, it must close the connection. It may then re-open the connection and try again.

## Extensions
> Read about the [general concept of extensions](./Language.md#extensions).

There are two ways of extending Punybuf, layers and extensions. Extensions are a softer approach and are usually preferred over layers.

### Extending structs
By default, all structs in Punybuf contain a single `UInt` at the end. The following struct...
```
Struct = {
	a_number: U32
	a_string: String
}
```
...must actually be represented like this:
```
      a_string
      v
{U32} {String} {UInt}
^              ^
a_number       extensions length (in octets)
```
By default, implementations must set the EL to `0` when serializing.  
When deserializing, implementations must read the EL, then consume that many extension octets ([Limits apply](#bytes)).

In the simplest case, when the implementation doesn't know of any extensions to the struct, it must discard all the octets consumed.

When the implementation is aware of some extensions, it must read all the extensions it's aware of (they are always in order) and discard all the other bytes that it doesn't understand.  
This could be done by either consuming all the extension octets at once and discarding everything after the expected extensions, or by reading all known extensions, decrementing the `extensions_length`, and then consuming & discarding `extensions_length` octets.

**If a struct has a flag field,**  
extensions are defined on that flag field by using the `@extension` attribute on the flag.

Note that simple boolean flags are [supported transparently](#flag-fields) (last paragraph) and don't require `@extension` attributes.
```
Struct = {
	a_number: U32
	a_string: String
	flags: U16.{
		predefined_flag?: U16
		boolean_flag?
		@extension
		some_bytes?: Bytes
	}
}
```
Without the extension flag set, this struct must be serialized like this (`predefined_flag` is also set for this example)
```
                              boolean_flag 
                              |
                              |predefined_flag
                              || |     EL (UInt)
                              vv v     v
{U32} {String} xxxxxxxx xxxxx001 {U16} 0
                             ^         ^
                             |         |
                             some_bytes (unset)
```

If the flag is set, the value of that flag must be put **after the EL boundary**, and the EL must be set to the length of that flag field:
```
                             some_bytes (set)
                             |             |
                             v             v
{U32} {String} xxxxxxxx xxxxx101 {U16} {n} {Bytes}
                                       ^   |--n--|
                                       EL
```
> `n` has to contain the *entire* length of `Bytes`, including the `UInt`, also representing the length.

To an outdated deserializer, this value will look like this:
```
                               predefined_flag
                               v v
{U32} {String} xxxxxxxx xxxxxx01 {U16} {n} {ignore this}
                             ^         ^   |-----n-----|
                             |         EL
                             unknown flag
                             (must ignore)
```

In the scope of one struct, the extension flags go in order of appearance.

```
Struct = {
	some_value: Value
	flags_1: U16.{
		predefined_flag_1?: String # immediately after this field
		predefined_flag_2?: String
			# after predefined_flag_1 of this flag field
		@extension
		ext_flag_1?: Value # immediately after the EL
		@extension
		ext_flag_2?: Value # after ext_flag_1
	}
	flags_2: U16.{
		predefined_flag_1_again?: String # immedately after this field
		@extension
		ext_flag_3?: Value
			# after ext_flag_2 from the previous flag field
		@extension
		ext_flag_4?: Value # after ext_flag_3
	}
}
```
The EL comprises the lengths of all set extensions.

**TODO: the following is not yet supported. Must implement checks for either flag field exhaustion, or no flags and convenience properties on the IR. Decide whether we allow multiple extension flag fields. Decide whether supporting this at all is worth it. Maybe only structs with flag fields may be extended until they are exhausted. Maybe only structs without flag fields may have `@extension_flags`.**

**If a struct doesn't have a flag field, or if it's exhausted,**  
the user defines an `@extension_flags` field that puts this flag field after the EL **AND** after all previous extensions on existing flag fields.

So, this...
```
Struct = {
	some_value: Value
	# exhausted flag field, no new extensions may be added
	flags_1: U8.{
		predef_flag_1?: String
		predef_flag_2?: String
		@extension
		ext_flag_1?: Value
		@extension
		ext_flag_2?: Value
		@extension
		ext_flag_3?: Value
		@extension
		ext_flag_4?: Value
		@extension
		ext_flag_5?: Value
		@extension
		ext_flag_6?: Value
	}
	@extension_flags
	flags_ext: U16.{
		more_extension_flag?: Value
	}
}
```
...must be serialized as this:
```
            ext_flag_2 -------+
            |                 |
            | predef_flag_2?  |              more_extension_flag?
            | |   |           |              |          |
            v v   v           v              v          v
{Value} 00001010 {String} {n} {Value} 00000001 00000000 {Value}
                              |---------------n---------------|
```
And to an outdated deserializer will look like this:
```
            ext_flag_2 -------+
            |                 |
            | predef_flag_2?  |
            | |   |           |
            v v   v           v
{Value} 00001010 {String} {n} {Value} {......ignore.this......}
                              |---------------n---------------|
```
**TODO: see above**  
If no `@extension_flags` are defined, implementations MUST NOT include an all-zero flags value after the EL, and instead must omit it entirely.  
This is to preserve the property of "one way to represent one value"

---

**Important**: structs marked as `@sealed` do not support extensions and don't have an EL (extra `UInt`) at the end.

Command arguments can also be structs, and this can also be extended in such way.

### Extending enums
Compared to structs, extending enums is very simple. If the user defined a `@default` variant, the enum is extensible. Otherwise, the enum is "sealed".

The default variant never has an associated value.

Extensions are then defined using the same `@extension` attribute.

If the discriminant is not an extension, the implementation must continue to (de)serialize the enum as normal.

When deserializing, if the discriminant is unknown, the implementation must read the `UInt` extension length value, discard that many bytes, then set the result to the `@default` value.  
If the discriminant is a known extension, the implementation should discard the `UInt` and read the value of the extension (if any).

When serializing, if the discriminant is an extension, serialize the value, if any, then encode its length as a `UInt`, then put the value right after the EL.

```
Mood = [
	@default
	Neutral, # = 0
	Happy, # = 1
	Sad, # = 2
	ThinkingAbout: String, # = 3 + Srting
	@extension
	ConfusedAbout: String, # = 4 + UInt + String
	@extension
	Hungry, # = 5 + UInt
]
```
A value of `Mood.ThinkingAbout({String})` (discriminant 3) would look like this, equivalent to if there were no extensions:
```
00000011 {String}
     = 3
```
However, a value of `Mood.ConfusedAbout({String})` (discriminant 4) would look like this:
```
00000100 {n} {String}
     = 4     |--n---|
```
which, to an outdated deserializer, would look like this:
```
00000100 {n} {ignore this}
     = 4     |-----n-----|
```
...who would set the resulting value to `Mood.Normal`.

A value of `Mood.Hungry` would look like this:
```
00000101 {0}
     = 5
```

## Layers
Okay, the complicated part is over. Layers are a sort of versioning system for the Punybuf RPC. They work transparently by just generating new commands and types whenever their dependencies change. This is done by the compiler at no additional cost to you.

There are a few recommendations, however:

1. In the [JSON IR](./Codegen.md), you will see that some commands' and types' names are duplicated. This is probably not allowed by your language, so you have to distinguish between them. The easiest way of doing it would be to just pre- or postfix these with something like `_Layer0`. However, try not to put affixes on everything, and instead make the latest layer contain the name verbatim.
2. Layer negotiation is out of scope for Punybuf RPC, but try to provide some ways of restricting the maximum layer if the negotiated layer is lower than the maximum supported one.