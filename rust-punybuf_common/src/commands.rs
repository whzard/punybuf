//! # Module for Punybuf commands.
//!
//! Enables support for Punybuf commands and tools to create generic RPC
//! systems.

use std::{io::{self, Read, Write}};

use crate::{PBSerialize, PBType};


/// A static trait to enable deserialization of commands.
/// 
/// The enum of all commands *does not* implement this trait.
///
/// Although all command arguments are deserializable like types,
/// because Punybuf doesn't allow you to put command arguments
/// inside other types, no commands implement
/// [`PBDeserialize`](crate::PBDeserialize).
pub trait PBCommandDeserialize<'x> {
	/// The return type of this command
	type Return<'a>: PBType<'a>;
	/// The error type of this command
	type Error<'a>: PBType<'a>;

	/// The ID of the command.
	const ID: u32;
	/// Whether the `Return` type is `Void`.
	const IS_VOID: bool = false;

	/// Contains all attributes defined on this command
	const ATTRIBUTES: &'static [(&'static str, Option<&'static str>)] = &[];
	/// Contains the value of the @capability attribute
	const REQUIRED_CAPABILITY: Option<&'static str> = None;

	/// Does **not** read the command ID.  
	/// If you need to read the command ID,
	/// use [`ProtocolCommands::deserialize_owned`].
	fn deserialize_owned<R: Read>(r: &mut R) -> io::Result<Self>
		where Self: Sized;
	/// Does **not** read the command ID.  
	/// If you need to read the command ID,
	/// use [`ProtocolCommands::deserialize`].
	fn deserialize<'a: 'x>(slice: &mut &'a[u8]) -> io::Result<Self>
		where Self: Sized
	{
		Self::deserialize_owned(slice)
	}
}

/// An interface trait to enable serialization of commands.
///
/// The enum of all commands also implements this trait.
///
/// Although all command arguments are serializable like types,
/// due to the sensitivity with ID serialization and because
/// Punybuf doesn't allow you to put command arguments inside other
/// types, no commands implement [`PBSerialize`].
pub trait PBCommandSerialize {
	/// The ID of the command.
	fn id(&self) -> u32;

	/// Whether the `Return` type is `Void`
	fn is_void(&self) -> bool { false }

	/// Contains all attributes defined on this command
	fn attributes(&self) -> &'static [(&'static str, Option<&'static str>)] { &[] }
	/// Returns the value of the @capability attribute
	fn required_capability(&self) -> Option<&'static str> {
		None
	}

	/// Does **not** write the command ID.
	fn serialize_self<W: Write>(&self, w: &mut W) -> io::Result<()>;

	/// Writes both the command ID and the argument body
	fn serialize<W: Write>(&self, w: &mut W) -> io::Result<()> {
		w.write_all(&self.id().to_be_bytes())?;
		self.serialize_self(w)
	}
}

/// Common trait that all individual commands implement.
///
/// This trait has a blanket implementation for all commands implementing
/// [`PBCommandSerialize`] and [`PBCommandDeserialize`].
pub trait PBCommand<'x>: PBCommandSerialize + PBCommandDeserialize<'x> {}

impl<'x, T> PBCommand<'x> for T
	where T: PBCommandSerialize + PBCommandDeserialize<'x> {}

/// A trait that one Punybuf protocol implements
pub trait PBProtocol {
	/// The enum of all possible command arguments in this protocol.
	type Commands<'a>: ProtocolCommands<'a>;
	/// The enum of all possible return values of the commands in this protocol.
	type Returns<'a>: ProtocolReturns<'a>;
	/// The enum of all possible error values of the commands in this protocol.
	type Errors<'a>: ProtocolErrors<'a>;
}

/// An enum of all supported commands (their IDs and arguments) in a protocol.
///
/// Note that you can serialize a value of this enum using the
/// [`PBCommandSerialize::serialize`] method, but to deserialize a command you
/// have to either use the [`deserialize`](Self::deserialize) associated
/// function, to read both the [`u32`] command ID and the argument, or use the
/// [`PBCommandDeserialize::deserialize`] (or `deserialize_owned`) of the
/// concrete command to only deserialize the argument.
pub trait ProtocolCommands<'x>: PBCommandSerialize {
	/// Expects to read the command ID as well as the argument.
	///
	/// For a version that only reads the argument, use
	/// [`PBCommandDeserialize::deserialize_owned`]
	fn deserialize_owned<R: Read>(r: &mut R) -> io::Result<Self>
		where Self: Sized;

	/// Expects to read the command ID as well as the argument
	///
	/// For a version that only reads the argument, use
	/// [`PBCommandDeserialize::deserialize`]
	fn deserialize<'a: 'x>(slice: &mut &'a[u8]) -> io::Result<Self>
		where Self: Sized
	{
		Self::deserialize_owned(slice)
	}
}

// TODO: could these two be the same trait? What do we call it?
// TODO: currently, the names for these methods clash with the names from
// `PBDeserialize`, even though their signatures are different.
// Should we rename them to `deserialize_id` or something?

/// An enum of all supported command return values in a protocol.
pub trait ProtocolReturns<'x>: PBSerialize {
	/// Deserializes the return value of a command with ID `id`.
	fn deserialize_owned<R: Read>(id: u32, r: &mut R) -> io::Result<Self>
		where Self: Sized;

	/// Deserializes the return value of a command with ID `id`.
	fn deserialize<'a: 'x>(id: u32, slice: &mut &'a[u8]) -> io::Result<Self>
		where Self: Sized
	{
		Self::deserialize_owned(id, slice)
	}

	/// Get the ID of the command that this return is for
	///
	/// Note: before release, this trait has a default implementation
	/// that always returns `0`.
	// TODO:1.0: make this required
	fn id(&self) -> u32 {
		0
	}
}

/// An enum of all supported command error values in a protocol.
pub trait ProtocolErrors<'x>: PBSerialize {
	/// Deserializes the error value of a command with ID `id`.
	fn deserialize_owned<R: Read>(id: u32, r: &mut R) -> io::Result<Self>
		where Self: Sized;

	/// Deserializes the error value of a command with ID `id`.
	fn deserialize<'a: 'x>(id: u32, slice: &mut &'a[u8]) -> io::Result<Self>
		where Self: Sized
	{
		Self::deserialize_owned(id, slice)
	}
	
	/// Get the ID of the command that this error is for
	///
	/// Note: before release, this trait has a default implementation
	/// that always returns `0`.
	// TODO:1.0: make this required
	fn id(&self) -> u32 {
		0
	}
}

/// A trait to check whether the passed command ID matches the
/// retained one.
///
/// For commands & errors, this just checks for equality of the inner ID
/// parameter and the passed ID. For return values, since multiple commands
/// may return the same thing, this checks if the command with the passed `id`
/// accepts returns of that type (as in, there might be more than one `id` that
/// fits).
///
/// Note that this trait has a blanket implementation for
/// [`PBProtocol::Commands`], but not other associated types since the compiler
/// cannot guarantee that these types don't overlap.
pub trait MatchID<Protocol: PBProtocol> {
	/// Check if this type can be used with id `id`.
	///
	/// This is a simple equality check for commands & errors, but for return
	/// values it checks if the command with id `id` returns this value.
	fn match_id(&self, id: u32) -> bool;
}

// This trait is only automatically implemented for P::Commands because
// otherwise we cannot guarantee that P::Commands, P::Errors, P::Returns
// are different types. This looks a little weird since one part is implemented
// here and the other in the generated definition.
impl<'any, P: PBProtocol> MatchID<P> for P::Commands<'any> {
	fn match_id(&self, id: u32) -> bool {
		self.id() == id
	}
}