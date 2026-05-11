use std::io::{self, Error};
use tokio::io::{AsyncReadExt, AsyncWriteExt};

pub use std::borrow::Cow;

use crate::{const_unwrap, from_utf8_lossy_owned};
pub use crate::{UInt, Done, Void, Bytes};

const MAX_BYTES_LENGTH: usize = const_unwrap!(usize::from_str_radix(env!("PUNYBUF_MAX_BYTES_LENGTH"), 10));
const MAX_ARRAY_LENGTH: usize = const_unwrap!(usize::from_str_radix(env!("PUNYBUF_MAX_ARRAY_LENGTH"), 10));

/// All Punybuf types implement this trait.
///
/// The lifetime arg on this trait is a leftover from the
/// non-tokio version of the punybuf library. Currently,
/// the tokio version does not support plain `deserialize`
/// methods, but maybe in the future it will. The lifetime
/// also allows for this change to potentially be non-
/// breaking.
pub trait PBType<'x>: Send + Sync {
	fn attributes() -> &'static [(&'static str, Option<&'static str>)] { &[] }
	fn serialize<W: AsyncWriteExt + Unpin + Send>(&self, w: &mut W) -> impl std::future::Future<Output = io::Result<()>> + Send;
	fn deserialize_stream<R: AsyncReadExt + Unpin + Send>(r: &mut R) -> impl std::future::Future<Output = io::Result<Self>> + Send where Self: Sized;
}

impl<'x> PBType<'x> for Done {
	async fn serialize<W: AsyncWriteExt + Unpin + Send>(&self, _w: &mut W) -> io::Result<()> {
		Ok(())
	}
	async fn deserialize_stream<R: AsyncReadExt + Unpin + Send>(_r: &mut R) -> io::Result<Self> {
		Ok(Self {})
	}
}

impl<'x> PBType<'x> for Void {
	async fn serialize<W: AsyncWriteExt + Unpin + Send>(&self, _: &mut W) -> io::Result<()> {
		Ok(())
	}
	async fn deserialize_stream<R: AsyncReadExt + Unpin + Send>(_: &mut R) -> io::Result<Self> {
		Ok(())
	}
}

impl<'x> PBType<'x> for UInt {
	async fn serialize<W: AsyncWriteExt + Unpin + Send>(&self, w: &mut W) -> io::Result<()> {
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
	async fn deserialize_stream<R: AsyncReadExt + Unpin + Send>(r: &mut R) -> io::Result<Self> {
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

impl<'x> PBType<'x> for u8 {
	async fn deserialize_stream<R: AsyncReadExt + Unpin + Send>(r: &mut R) -> io::Result<Self> {
		let mut buf = [0; 1];
		r.read_exact(&mut buf).await?;
		Ok(buf[0])
	}
	async fn serialize<W: AsyncWriteExt + Unpin + Send>(&self, w: &mut W) -> io::Result<()> {
		w.write_all(&[*self]).await
	}
}
impl<'x> PBType<'x> for u16 {
	async fn deserialize_stream<R: AsyncReadExt + Unpin + Send>(r: &mut R) -> io::Result<Self> {
		let mut buf = [0; 2];
		r.read_exact(&mut buf).await?;
		Ok(Self::from_be_bytes(buf))
	}
	async fn serialize<W: AsyncWriteExt + Unpin + Send>(&self, w: &mut W) -> io::Result<()> {
		w.write_all(&self.to_be_bytes()).await
	}
}
impl<'x> PBType<'x> for u32 {
	async fn deserialize_stream<R: AsyncReadExt + Unpin + Send>(r: &mut R) -> io::Result<Self> {
		let mut buf = [0; 4];
		r.read_exact(&mut buf).await?;
		Ok(Self::from_be_bytes(buf))
	}
	async fn serialize<W: AsyncWriteExt + Unpin + Send>(&self, w: &mut W) -> io::Result<()> {
		w.write_all(&self.to_be_bytes()).await
	}
}
impl<'x> PBType<'x> for u64 {
	async fn deserialize_stream<R: AsyncReadExt + Unpin + Send>(r: &mut R) -> io::Result<Self> {
		let mut buf = [0; 8];
		r.read_exact(&mut buf).await?;
		Ok(Self::from_be_bytes(buf))
	}
	async fn serialize<W: AsyncWriteExt + Unpin + Send>(&self, w: &mut W) -> io::Result<()> {
		w.write_all(&self.to_be_bytes()).await
	}
}
impl<'x> PBType<'x> for i32 {
	async fn deserialize_stream<R: AsyncReadExt + Unpin + Send>(r: &mut R) -> io::Result<Self> {
		let mut buf = [0; 4];
		r.read_exact(&mut buf).await?;
		Ok(Self::from_be_bytes(buf))
	}
	async fn serialize<W: AsyncWriteExt + Unpin + Send>(&self, w: &mut W) -> io::Result<()> {
		w.write_all(&self.to_be_bytes()).await
	}
}
impl<'x> PBType<'x> for i64 {
	async fn deserialize_stream<R: AsyncReadExt + Unpin + Send>(r: &mut R) -> io::Result<Self> {
		let mut buf = [0; 8];
		r.read_exact(&mut buf).await?;
		Ok(Self::from_be_bytes(buf))
	}
	async fn serialize<W: AsyncWriteExt + Unpin + Send>(&self, w: &mut W) -> io::Result<()> {
		w.write_all(&self.to_be_bytes()).await
	}
}
impl<'x> PBType<'x> for f32 {
	async fn deserialize_stream<R: AsyncReadExt + Unpin + Send>(r: &mut R) -> io::Result<Self> {
		let mut buf = [0; 4];
		r.read_exact(&mut buf).await?;
		Ok(Self::from_be_bytes(buf))
	}
	async fn serialize<W: AsyncWriteExt + Unpin + Send>(&self, w: &mut W) -> io::Result<()> {
		w.write_all(&self.to_be_bytes()).await
	}
}
impl<'x> PBType<'x> for f64 {
	async fn deserialize_stream<R: AsyncReadExt + Unpin + Send>(r: &mut R) -> io::Result<Self> {
		let mut buf = [0; 8];
		r.read_exact(&mut buf).await?;
		Ok(Self::from_be_bytes(buf))
	}
	async fn serialize<W: AsyncWriteExt + Unpin + Send>(&self, w: &mut W) -> io::Result<()> {
		w.write_all(&self.to_be_bytes()).await
	}
}

impl<'x, T: PBType<'x>> PBType<'x> for Vec<T> {
	async fn serialize<W: AsyncWriteExt + Unpin + Send>(&self, w: &mut W) -> io::Result<()> {
		let len = self.len() as u64;
		UInt(len).serialize(w).await?;
		for item in self {
			item.serialize(w).await?;
		}
		Ok(())
	}
	async fn deserialize_stream<R: AsyncReadExt + Unpin + Send>(r: &mut R) -> io::Result<Self> {
		let len = UInt::deserialize_stream(r).await?.into();
		if len > MAX_ARRAY_LENGTH {
			return Err(Error::other("Array length too large"));
		}
		let mut this = Vec::with_capacity(len);

		for _ in 0..len {
			this.push(T::deserialize_stream(r).await?);
		}

		Ok(this)
	}
}

impl<'x> PBType<'x> for Bytes<'_> {
	async fn serialize<W: AsyncWriteExt + Unpin + Send>(&self, w: &mut W) -> io::Result<()> {
		let len = self.0.len() as u64;
		UInt(len).serialize(w).await?;
		w.write_all(&self.0).await?;
		Ok(())
	}
	async fn deserialize_stream<R: AsyncReadExt + Unpin + Send>(r: &mut R) -> io::Result<Self> {
		let len = UInt::deserialize_stream(r).await?.into();
		if len > MAX_BYTES_LENGTH {
			return Err(Error::other("Bytes length too large"));
		}
		let mut this = Vec::with_capacity(len);
		let mut taken = r.take(len as u64);

		taken.read_to_end(&mut this).await?;
		Ok(Self(this.into()))
	}
}


impl<'x> PBType<'x> for String {
	async fn deserialize_stream<R: AsyncReadExt + Unpin + Send>(r: &mut R) -> io::Result<Self> {
		let len = UInt::deserialize_stream(r).await?.into();
		if len > MAX_BYTES_LENGTH {
			return Err(Error::other("String length too large"));
		}

		let mut this = Vec::with_capacity(len);
		let mut taken = r.take(len as u64);

		taken.read_to_end(&mut this).await?;

		Ok(from_utf8_lossy_owned(this))
	}
	async fn serialize<W: AsyncWriteExt + Unpin + Send>(&self, w: &mut W) -> io::Result<()> {
		let len = self.len() as u64;
		UInt(len).serialize(w).await?;
		w.write_all(self.as_bytes()).await?;
		Ok(())
	}
}

impl<'x> PBType<'x> for Cow<'_, str> {
	async fn deserialize_stream<R: AsyncReadExt + Unpin + Send>(r: &mut R) -> io::Result<Self> {
		Ok(String::deserialize_stream(r).await?.into())
	}

	async fn serialize<W: AsyncWriteExt + Unpin + Send>(&self, w: &mut W) -> io::Result<()> {
		self.to_string().serialize(w).await
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
pub trait PBCommandExt<'x>: Sized + Send + Sync {
	type Error<'a>: PBType<'a>;
	type Return<'a>: PBType<'a>;
	/// The ID of the command.
	const ID: u32;
	/// Whether the `Return` type is `Void`.
	const IS_VOID: bool = false;

	const ATTRIBUTES: &'static [(&'static str, Option<&'static str>)] = &[];
	const REQUIRED_CAPABILITY: Option<&'static str> = None;

	fn deserialize_return_stream<R: AsyncReadExt + Unpin + Send>(&self, r: &mut R) -> impl std::future::Future<Output = io::Result<Self::Return<'static>>> + Send {
		async { Self::Return::deserialize_stream(r).await }
	}
	fn deserialize_error_stream<R: AsyncReadExt + Unpin + Send>(&self, r: &mut R) -> impl std::future::Future<Output = io::Result<Self::Error<'static>>> + Send {
		async { Self::Error::deserialize_stream(r).await }
	}

	/// Does **not** read the command ID.  
	/// If you need to read the command ID, use `Command::deserialize` from the generated file.
	fn deserialize_stream<R: AsyncReadExt + Unpin + Send>(r: &mut R) -> impl std::future::Future<Output = io::Result<Self>> + Send;
}

/// A trait that all commands implement.
pub trait PBCommand: Sized + Send + Sync {
	fn id(&self) -> u32;

	/// Whether the `Return` type is `Void`
	fn is_void(&self) -> bool { false }

	fn attributes(&self) -> &'static [(&'static str, Option<&'static str>)] { &[] }
	fn required_capability(&self) -> Option<&'static str> {
		None
	}

	/// Does **not** write the command ID.
	fn serialize_self<W: AsyncWriteExt + Unpin + Send>(&self, w: &mut W) -> impl std::future::Future<Output = io::Result<()>> + Send;

	/// Writes both the command ID and the argument body
	fn serialize<W: AsyncWriteExt + Unpin + Send>(&self, w: &mut W) -> impl std::future::Future<Output = io::Result<()>> + Send {
		async {
			w.write_all(&self.id().to_be_bytes()).await?;
			self.serialize_self(w).await
		}
	}
}