# Punybuf
Punybuf is a strongly typed data serialization format. [Read more about it here](https://github.com/whzard/punybuf).

This crate implements the `common` types in punybuf, so that the generated code can link to this crate.

*This isn't yet 1.0 - the API isn't fully realized yet and this crate may contain bugs. Until this crate reaches 1.0 the changes may be made in a backwards-incompatible way*

All types may be deserialized from streams.

## Architecture
The generated types implement the [`PBSerialize`] and [`PBDeserialize`] traits, which, due to the blanket impl, also implement [`PBType`].

Generic parameters in Punybuf types always take a `PBType` (so something that impls both traits).

Punybuf makes heavy use of [`std::borrow::Cow`] to support both zero-copy and owned deserialization in sync contexts. To support both, you use [`PBDeserialize::deserialize`] for zero-copy and [`PBDeserialize::deserialize_owned`] for owned.

## Commands
The `pbd` tool generates types and traits for the types specified in the definition. It generates one Rust type per Punybuf type and two types per command, one for the argument, and one for the error type.

The `pbd` tool also generates an unconstructable type implementing the [`commands::PBProtocol`] trait.

Its [`commands::PBProtocol::Commands`] is an enum of all possible commands in the protocol, that implements [`commands::ProtocolCommands`] (& [`commands::PBCommandSerialize`] because it's an interface trait, but not [`commands::PBCommandDeserialize`], since the sematics are different and we need to also read the command ID to deserialize it).

The protocol type has [`commands::PBProtocol::Errors`] (impl [`commands::ProtocolErrors`]) and [`commands::PBProtocol::Returns`] (impls [`commands::ProtocolReturns`]), two more enums containing all possible error or return values per each command. Moreover, some of the generated definitions contain [`From`] and [`TryFrom`] implementations to convert between enums and concrete types.

Another notable trait is [`commands::MatchID`], which allows for checking whether a type can be used with a command id (as a return value or as an error value). *Unfortunately, there isn't a way for this check to be made during compilation time.*

Utilities implementing an RPC system can take advantage of these traits to create generic strongly typed functions.