use std::io::{self, Error};
use tokio::io::{AsyncReadExt, AsyncWriteExt};

pub use std::borrow::Cow;

use crate::{MAX_ARRAY_LENGTH, MAX_BYTES_LENGTH};
pub use crate::{UInt, Done, Void, Bytes, PBCommand, PBCommandExt, PBType};

/// All Punybuf types implement this trait.
///
/// This trait alone does not support getting attributes,
/// but in practice, all Punybuf types will implement
/// `PBType` (non-async), which does support attributes
pub trait AsyncPBType: Send + Sync {
	fn serialize_async<W: AsyncWriteExt + Unpin + Send>(&self, w: &mut W) -> impl std::future::Future<Output = io::Result<()>> + Send;
	fn deserialize_async<R: AsyncReadExt + Unpin + Send>(r: &mut R) -> impl std::future::Future<Output = io::Result<Self>> + Send where Self: Sized;
}

impl AsyncPBType for Done {
	async fn serialize_async<W: AsyncWriteExt + Unpin + Send>(&self, _w: &mut W) -> io::Result<()> {
		Ok(())
	}
	async fn deserialize_async<R: AsyncReadExt + Unpin + Send>(_r: &mut R) -> io::Result<Self> {
		Ok(Self {})
	}
}

impl AsyncPBType for Void {
	async fn serialize_async<W: AsyncWriteExt + Unpin + Send>(&self, _: &mut W) -> io::Result<()> {
		Ok(())
	}
	async fn deserialize_async<R: AsyncReadExt + Unpin + Send>(_: &mut R) -> io::Result<Self> {
		Ok(())
	}
}

impl AsyncPBType for UInt {
	async fn serialize_async<W: AsyncWriteExt + Unpin + Send>(&self, w: &mut W) -> io::Result<()> {
		let mut uint = self.0;
		if uint < 128 {
			w.write_all(&uint.to_be_bytes()[7..8]).await?;

			} else if uint < 16512 {
				uint -= 128;
				let bytes = &mut uint.to_be_bytes()[6..8];
				bytes[0] |= 0b10_000000;
				w.write_all(bytes).await?;

			} else if uint < 2113664 {
				uint -= 16512;
				let bytes = &mut uint.to_be_bytes()[5..8];
				bytes[0] |= 0b110_00000;
				w.write_all(bytes).await?;

			} else if uint < 68721590400 {
				uint -= 2113664;
				let bytes = &mut uint.to_be_bytes()[3..8];
				bytes[0] |= 0b1110_0000;
				w.write_all(bytes).await?;

			} else if uint < 1152921573328437376 {
				uint -= 68721590400;
				let bytes = &mut uint.to_be_bytes()[0..8];
				bytes[0] |= 0b1111_0000;
				w.write_all(bytes).await?;

			} else {
				Err(io::Error::other("number too big (max 1152921573328437375)"))?;
			}
			Ok(())
	}
	async fn deserialize_async<R: AsyncReadExt + Unpin + Send>(r: &mut R) -> io::Result<Self> {
		let mut first_byte = [0; 1];
		r.read_exact(&mut first_byte).await?;
		
		let mut buf = [0; 8];
		let first_byte = first_byte[0];
		buf[0] = first_byte;
		Ok(
			if first_byte >> 7 == 0 {
				// 0xxxxxxx
				Self(u64::from(first_byte))

			} else if first_byte & 0b010_00000 == 0 {
				// 10xxxxxx
				buf[0] &= 0b00_111111;
				r.read_exact(&mut buf[1..2]).await?;
				Self(u64::from_le_bytes([buf[1], buf[0], 0, 0, 0, 0, 0, 0]) + 128)

			} else if first_byte & 0b001_00000 == 0 {
				// 110xxxxx
				buf[0] &= 0b000_11111;
				r.read_exact(&mut buf[1..3]).await?;
				Self(u64::from_le_bytes([buf[2], buf[1], buf[0], 0, 0, 0, 0, 0]) + 16512)

			} else if first_byte & 0b0001_0000 == 0 {
				// 1110xxxx
				buf[0] &= 0b0000_1111;
				r.read_exact(&mut buf[1..5]).await?;
				Self(u64::from_le_bytes([buf[4], buf[3], buf[2], buf[1], buf[0], 0, 0, 0]) + 2113664)

			} else {
				// 1111xxxx
				buf[0] &= 0b0000_1111;
				r.read_exact(&mut buf[1..8]).await?;
				Self(u64::from_le_bytes([buf[7], buf[6], buf[5], buf[4], buf[3], buf[2], buf[1], buf[0]]) + 68721590400)
			}
		)
	}
}

impl AsyncPBType for u8 {
	async fn deserialize_async<R: AsyncReadExt + Unpin + Send>(r: &mut R) -> io::Result<Self> {
		let mut buf = [0; 1];
		r.read_exact(&mut buf).await?;
		Ok(buf[0])
	}
	async fn serialize_async<W: AsyncWriteExt + Unpin + Send>(&self, w: &mut W) -> io::Result<()> {
		w.write_all(&[*self]).await
	}
}
impl AsyncPBType for u16 {
	async fn deserialize_async<R: AsyncReadExt + Unpin + Send>(r: &mut R) -> io::Result<Self> {
		let mut buf = [0; 2];
		r.read_exact(&mut buf).await?;
		Ok(Self::from_be_bytes(buf))
	}
	async fn serialize_async<W: AsyncWriteExt + Unpin + Send>(&self, w: &mut W) -> io::Result<()> {
		w.write_all(&self.to_be_bytes()).await
	}
}
impl AsyncPBType for u32 {
	async fn deserialize_async<R: AsyncReadExt + Unpin + Send>(r: &mut R) -> io::Result<Self> {
		let mut buf = [0; 4];
		r.read_exact(&mut buf).await?;
		Ok(Self::from_be_bytes(buf))
	}
	async fn serialize_async<W: AsyncWriteExt + Unpin + Send>(&self, w: &mut W) -> io::Result<()> {
		w.write_all(&self.to_be_bytes()).await
	}
}
impl AsyncPBType for u64 {
	async fn deserialize_async<R: AsyncReadExt + Unpin + Send>(r: &mut R) -> io::Result<Self> {
		let mut buf = [0; 8];
		r.read_exact(&mut buf).await?;
		Ok(Self::from_be_bytes(buf))
	}
	async fn serialize_async<W: AsyncWriteExt + Unpin + Send>(&self, w: &mut W) -> io::Result<()> {
		w.write_all(&self.to_be_bytes()).await
	}
}
impl AsyncPBType for i32 {
	async fn deserialize_async<R: AsyncReadExt + Unpin + Send>(r: &mut R) -> io::Result<Self> {
		let mut buf = [0; 4];
		r.read_exact(&mut buf).await?;
		Ok(Self::from_be_bytes(buf))
	}
	async fn serialize_async<W: AsyncWriteExt + Unpin + Send>(&self, w: &mut W) -> io::Result<()> {
		w.write_all(&self.to_be_bytes()).await
	}
}
impl AsyncPBType for i64 {
	async fn deserialize_async<R: AsyncReadExt + Unpin + Send>(r: &mut R) -> io::Result<Self> {
		let mut buf = [0; 8];
		r.read_exact(&mut buf).await?;
		Ok(Self::from_be_bytes(buf))
	}
	async fn serialize_async<W: AsyncWriteExt + Unpin + Send>(&self, w: &mut W) -> io::Result<()> {
		w.write_all(&self.to_be_bytes()).await
	}
}
impl AsyncPBType for f32 {
	async fn deserialize_async<R: AsyncReadExt + Unpin + Send>(r: &mut R) -> io::Result<Self> {
		let mut buf = [0; 4];
		r.read_exact(&mut buf).await?;
		Ok(Self::from_be_bytes(buf))
	}
	async fn serialize_async<W: AsyncWriteExt + Unpin + Send>(&self, w: &mut W) -> io::Result<()> {
		w.write_all(&self.to_be_bytes()).await
	}
}
impl AsyncPBType for f64 {
	async fn deserialize_async<R: AsyncReadExt + Unpin + Send>(r: &mut R) -> io::Result<Self> {
		let mut buf = [0; 8];
		r.read_exact(&mut buf).await?;
		Ok(Self::from_be_bytes(buf))
	}
	async fn serialize_async<W: AsyncWriteExt + Unpin + Send>(&self, w: &mut W) -> io::Result<()> {
		w.write_all(&self.to_be_bytes()).await
	}
}

impl<T: AsyncPBType> AsyncPBType for Vec<T> {
	async fn serialize_async<W: AsyncWriteExt + Unpin + Send>(&self, w: &mut W) -> io::Result<()> {
		let len = self.len() as u64;
		UInt(len).serialize_async(w).await?;
		for item in self {
			item.serialize_async(w).await?;
		}
		Ok(())
	}
	async fn deserialize_async<R: AsyncReadExt + Unpin + Send>(r: &mut R) -> io::Result<Self> {
		let len = UInt::deserialize_async(r).await?.into();
		if len > MAX_ARRAY_LENGTH {
			return Err(Error::other("Array length too large"));
		}
		let mut this = Vec::with_capacity(len);

		for _ in 0..len {
			this.push(T::deserialize_async(r).await?);
		}

		Ok(this)
	}
}

impl AsyncPBType for Bytes<'_> {
	async fn serialize_async<W: AsyncWriteExt + Unpin + Send>(&self, w: &mut W) -> io::Result<()> {
		let len = self.0.len() as u64;
		UInt(len).serialize_async(w).await?;
		w.write_all(&self.0).await?;
		Ok(())
	}
	async fn deserialize_async<R: AsyncReadExt + Unpin + Send>(r: &mut R) -> io::Result<Self> {
		let len = UInt::deserialize_async(r).await?.into();
		if len > MAX_BYTES_LENGTH {
			return Err(Error::other("Bytes length too large"));
		}
		let mut this = Vec::with_capacity(len);
		let mut taken = r.take(len as u64);

		taken.read_to_end(&mut this).await?;
		Ok(Self(this.into()))
	}
}


impl AsyncPBType for String {
	async fn deserialize_async<R: AsyncReadExt + Unpin + Send>(r: &mut R) -> io::Result<Self> {
		let len = UInt::deserialize_async(r).await?.into();
		if len > MAX_BYTES_LENGTH {
			return Err(Error::other("String length too large"));
		}

		let mut this = Vec::with_capacity(len);
		let mut taken = r.take(len as u64);

		taken.read_to_end(&mut this).await?;

		Ok(String::from_utf8(this).map_err(|e|
			io::Error::new(io::ErrorKind::InvalidData, e)
		)?)
	}
	async fn serialize_async<W: AsyncWriteExt + Unpin + Send>(&self, w: &mut W) -> io::Result<()> {
		let len = self.len() as u64;
		UInt(len).serialize_async(w).await?;
		w.write_all(self.as_bytes()).await?;
		Ok(())
	}
}

impl AsyncPBType for Cow<'_, str> {
	async fn deserialize_async<R: AsyncReadExt + Unpin + Send>(r: &mut R) -> io::Result<Self> {
		Ok(String::deserialize_async(r).await?.into())
	}

	async fn serialize_async<W: AsyncWriteExt + Unpin + Send>(&self, w: &mut W) -> io::Result<()> {
		self.to_string().serialize_async(w).await
	}
}


/// A trait that all individual commands implement. The enum of all commands *does not* implement this trait.
///
/// The lifetime arg on this trait is a leftover from the
/// non-tokio version of the punybuf library. Currently,
/// the tokio version does not support plain `deserialize`
/// methods, but maybe in the future it will. The lifetime
/// also allows for this change to potentially be non-
/// breaking.
pub trait AsyncPBCommandExt: Sized + Send + Sync {
	type AsyncError: AsyncPBType;
	type AsyncReturn: AsyncPBType;

	fn deserialize_return_async<R: AsyncReadExt + Unpin + Send>(&self, r: &mut R) -> impl std::future::Future<Output = io::Result<Self::AsyncReturn>> + Send {
		async { Self::AsyncReturn::deserialize_async(r).await }
	}
	fn deserialize_error_async<R: AsyncReadExt + Unpin + Send>(&self, r: &mut R) -> impl std::future::Future<Output = io::Result<Self::AsyncError>> + Send {
		async { Self::AsyncError::deserialize_async(r).await }
	}

	/// Does **not** read the command ID.  
	/// If you need to read the command ID, use `Command::deserialize` from the generated file.
	fn deserialize_async<R: AsyncReadExt + Unpin + Send>(r: &mut R) -> impl std::future::Future<Output = io::Result<Self>> + Send;
}

/// A trait that all commands implement.
pub trait AsyncPBCommand: Sized + Send + Sync + PBCommand {
	/// Does **not** write the command ID.
	fn serialize_self_async<W: AsyncWriteExt + Unpin + Send>(&self, w: &mut W) -> impl std::future::Future<Output = io::Result<()>> + Send;

	/// Writes both the command ID and the argument body
	fn serialize_async<W: AsyncWriteExt + Unpin + Send>(&self, w: &mut W) -> impl std::future::Future<Output = io::Result<()>> + Send {
		async {
			w.write_all(&self.id().to_be_bytes()).await?;
			self.serialize_self_async(w).await
		}
	}
}