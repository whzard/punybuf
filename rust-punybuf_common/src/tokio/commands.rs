use std::io;

use tokio::io::{AsyncReadExt, AsyncWriteExt};

use crate::{commands::PBCommandSerialize, tokio::{AsyncPBDeserialize, AsyncPBSerialize, AsyncPBType}};


/// A trait that all individual commands implement.
///
/// The enum of all commands *does not* implement this trait.
///
/// See [`PBCommandDeserialize`](crate::commands::PBCommandDeserialize).
pub trait AsyncPBCommandDeserialize: Sized + Send + Sync {
	/// The return type of this command.
	type AsyncReturn: AsyncPBType;
	/// The error type of this command.
	type AsyncError: AsyncPBType;

	fn deserialize_return_async<R: AsyncReadExt + Unpin + Send>(&self, r: &mut R) ->
		impl std::future::Future<Output = io::Result<Self::AsyncReturn>> + Send
	{
		async { Self::AsyncReturn::deserialize_async(r).await }
	}
	fn deserialize_error_async<R: AsyncReadExt + Unpin + Send>(&self, r: &mut R) ->
		impl std::future::Future<Output = io::Result<Self::AsyncError>> + Send
	{
		async { Self::AsyncError::deserialize_async(r).await }
	}

	/// Does **not** read the command ID.  
	/// If you need to read the command ID,
	/// use [`AsyncProtocolCommands::deserialize_async`].
	fn deserialize_async<R: AsyncReadExt + Unpin + Send>(r: &mut R) ->
		impl std::future::Future<Output = io::Result<Self>> + Send;
}

/// A trait that all commands implement.
///
/// The enum of all commands also implements this trait.
///
/// See [`PBCommandSerialize`].
pub trait AsyncPBCommandSerialize: Sized + Send + Sync + PBCommandSerialize {
	/// Does **not** write the command ID.
	fn serialize_self_async<W: AsyncWriteExt + Unpin + Send>(&self, w: &mut W) ->
		impl std::future::Future<Output = io::Result<()>> + Send;

	/// Writes both the command ID and the argument body
	fn serialize_async<W: AsyncWriteExt + Unpin + Send>(&self, w: &mut W) ->
		impl std::future::Future<Output = io::Result<()>> + Send
	{
		async {
			w.write_all(&self.id().to_be_bytes()).await?;
			self.serialize_self_async(w).await
		}
	}
}

/// Common trait that all individual commands implement.
///
/// This trait has a blanket implementation for all commands implementing
/// [`AsyncPBCommandSerialize`] and [`AsyncPBCommandDeserialize`]
pub trait AsyncPBCommand: AsyncPBCommandSerialize + AsyncPBCommandDeserialize {}

impl<T> AsyncPBCommand for T where T: AsyncPBCommandSerialize + AsyncPBCommandDeserialize {}

/// A trait that the enum of all known commands in a protocol implements.
pub trait AsyncPBProtocol {
	/// The enum of all possible command arguments in this protocol.
	type AsyncCommands: AsyncProtocolCommands;
	/// The enum of all possible return values of the commands in this protocol.
	type AsyncReturns: AsyncProtocolReturns;
	/// The enum of all possible error values of the commands in this protocol.
	type AsyncErrors: AsyncProtocolErrors;
}

/// An enum of all supported commands (their IDs and arguments) in a protocol.
///
/// See [`ProtocolCommands`](crate::commands::ProtocolCommands).
pub trait AsyncProtocolCommands: AsyncPBCommandSerialize {
	/// Expects to read the command ID as well as the argument.
	///
	/// For a version that only reads the argument, use
	/// [`AsyncPBCommandDeserialize::deserialize_async`]
	fn deserialize_async<R: AsyncReadExt + Unpin + Send>(r: &mut R) ->
		impl std::future::Future<Output = io::Result<Self>> + Send
		where Self: Sized;
}

/// An enum of all supported command return values in a protocol.
pub trait AsyncProtocolReturns: AsyncPBSerialize {
	/// Deserializes the return value of a command with ID `id`.
	fn deserialize_async<R: AsyncReadExt + Unpin + Send>(id: u32, r: &mut R) ->
		impl std::future::Future<Output = io::Result<Self>> + Send
		where Self: Sized;
}

/// An enum of all supported command error values in a protocol.
pub trait AsyncProtocolErrors: AsyncPBSerialize {
	/// Deserializes the error value of a command with ID `id`.
	fn deserialize_async<R: AsyncReadExt + Unpin + Send>(id: u32, r: &mut R) ->
		impl std::future::Future<Output = io::Result<Self>> + Send
		where Self: Sized;
}