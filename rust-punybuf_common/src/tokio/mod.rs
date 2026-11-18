//! Optional async (de)serialization support.

use std::io::{self, Error};
use std::ops::Deref;
use tokio::io::{AsyncReadExt, AsyncWriteExt};

use std::borrow::Cow;

use crate::{
	KeyPair, MAX_ARRAY_LENGTH, MAX_BYTES_LENGTH,
	UInt, Done, Void, Bytes
};

pub mod commands;

/// Contains all items that the generated definitions must import.
///
/// # Stability
/// It's not advisable to import this in user code, since items may
/// be added without incrementing the major version, potentially causing
/// conflicts.
pub mod prelude {
	pub use crate::tokio::{
		AsyncPBType, AsyncPBSerialize, AsyncPBDeserialize,
		commands::{
			AsyncPBCommand, AsyncPBCommandDeserialize, AsyncPBCommandSerialize,
			AsyncPBProtocol, AsyncProtocolCommands,
			AsyncProtocolErrors, AsyncProtocolReturns,
		}
	};
}

/// A trait to enable async serialization of Punybuf types.
///
/// Note: this trait alone isn't enough to get attributes of a specific
/// type. You should use the [`PBSerialize`](crate::PBSerialize) (non-async)
/// for that.
pub trait AsyncPBSerialize: Send + Sync {
	fn serialize_async<'a, W: AsyncWriteExt + Unpin + Send>(&'a self, w: &'a mut W) ->
		impl std::future::Future<Output = io::Result<()>> + Send + 'a;
}

/// A trait to enable async deserialization of Punybuf types.
///
/// Note: this trait alone isn't enough to get attributes of a specific
/// type. You should use the [`PBDeserialize`](crate::PBDeserialize)
/// (non-async) for that. That trait expects a lifetime annotation, passing
/// `'static` should be fine.
pub trait AsyncPBDeserialize: Send + Sync {
	fn deserialize_async<R: AsyncReadExt + Unpin + Send>(r: &mut R) ->
		impl std::future::Future<Output = io::Result<Self>> + Send
		where Self: Sized;
}


/// Common trait that all types implement.
///
/// This trait has a blanket implementation for all types implementing
/// [`AsyncPBSerialize`] and [`AsyncPBDeserialize`].
pub trait AsyncPBType: AsyncPBSerialize + AsyncPBDeserialize {}

impl<T> AsyncPBType for T where T: AsyncPBSerialize + AsyncPBDeserialize {}

impl<T> AsyncPBSerialize for Box<T> where T: AsyncPBSerialize {
	fn serialize_async<'a, W: AsyncWriteExt + Unpin + Send>(&'a self, w: &'a mut W) ->
		impl std::future::Future<Output = io::Result<()>> + Send + 'a
	{
		self.deref().serialize_async(w)
	}
}
impl<T> AsyncPBDeserialize for Box<T> where T: AsyncPBDeserialize {
	async fn deserialize_async<R: AsyncReadExt + Unpin + Send>(r: &mut R) ->
		io::Result<Self> where Self: Sized
	{
		T::deserialize_async(r).await.map(|x| Box::new(x))
	}
}

impl AsyncPBSerialize for Done {
	async fn serialize_async<W: AsyncWriteExt + Unpin + Send>(&self, _w: &mut W) ->
		io::Result<()>
	{
		Ok(())
	}
}
impl AsyncPBDeserialize for Done {
	async fn deserialize_async<R: AsyncReadExt + Unpin + Send>(_r: &mut R) ->
		io::Result<Self>
	{
		Ok(Self {})
	}
}

impl AsyncPBSerialize for Void {
	async fn serialize_async<W: AsyncWriteExt + Unpin + Send>(&self, _: &mut W) ->
		io::Result<()>
	{
		Ok(())
	}
}
impl AsyncPBDeserialize for Void {
	async fn deserialize_async<R: AsyncReadExt + Unpin + Send>(_: &mut R) ->
		io::Result<Self>
	{
		Ok(())
	}
}

impl AsyncPBSerialize for UInt {
	async fn serialize_async<W: AsyncWriteExt + Unpin + Send>(&self, w: &mut W) ->
		io::Result<()>
	{
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
}
impl AsyncPBDeserialize for UInt {
	async fn deserialize_async<R: AsyncReadExt + Unpin + Send>(r: &mut R) ->
		io::Result<Self>
	{
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

impl AsyncPBDeserialize for u8 {
	async fn deserialize_async<R: AsyncReadExt + Unpin + Send>(r: &mut R) ->
		io::Result<Self>
	{
		let mut buf = [0; 1];
		r.read_exact(&mut buf).await?;
		Ok(buf[0])
	}
}
impl AsyncPBSerialize for u8 {
	async fn serialize_async<W: AsyncWriteExt + Unpin + Send>(&self, w: &mut W) ->
		io::Result<()>
	{
		w.write_all(&[*self]).await
	}
}
impl AsyncPBDeserialize for u16 {
	async fn deserialize_async<R: AsyncReadExt + Unpin + Send>(r: &mut R) ->
		io::Result<Self>
	{
		let mut buf = [0; 2];
		r.read_exact(&mut buf).await?;
		Ok(Self::from_be_bytes(buf))
	}
}
impl AsyncPBSerialize for u16 {
	async fn serialize_async<W: AsyncWriteExt + Unpin + Send>(&self, w: &mut W) ->
		io::Result<()>
	{
		w.write_all(&self.to_be_bytes()).await
	}
}
impl AsyncPBDeserialize for u32 {
	async fn deserialize_async<R: AsyncReadExt + Unpin + Send>(r: &mut R) ->
		io::Result<Self>
	{
		let mut buf = [0; 4];
		r.read_exact(&mut buf).await?;
		Ok(Self::from_be_bytes(buf))
	}
}
impl AsyncPBSerialize for u32 {
	async fn serialize_async<W: AsyncWriteExt + Unpin + Send>(&self, w: &mut W) ->
		io::Result<()>
	{
		w.write_all(&self.to_be_bytes()).await
	}
}
impl AsyncPBDeserialize for u64 {
	async fn deserialize_async<R: AsyncReadExt + Unpin + Send>(r: &mut R) ->
		io::Result<Self>
	{
		let mut buf = [0; 8];
		r.read_exact(&mut buf).await?;
		Ok(Self::from_be_bytes(buf))
	}
}
impl AsyncPBSerialize for u64 {
	async fn serialize_async<W: AsyncWriteExt + Unpin + Send>(&self, w: &mut W) ->
		io::Result<()>
	{
		w.write_all(&self.to_be_bytes()).await
	}
}
impl AsyncPBDeserialize for i32 {
	async fn deserialize_async<R: AsyncReadExt + Unpin + Send>(r: &mut R) ->
		io::Result<Self>
	{
		let mut buf = [0; 4];
		r.read_exact(&mut buf).await?;
		Ok(Self::from_be_bytes(buf))
	}
}
impl AsyncPBSerialize for i32 {
	async fn serialize_async<W: AsyncWriteExt + Unpin + Send>(&self, w: &mut W) ->
		io::Result<()>
	{
		w.write_all(&self.to_be_bytes()).await
	}
}
impl AsyncPBDeserialize for i64 {
	async fn deserialize_async<R: AsyncReadExt + Unpin + Send>(r: &mut R) ->
		io::Result<Self>
	{
		let mut buf = [0; 8];
		r.read_exact(&mut buf).await?;
		Ok(Self::from_be_bytes(buf))
	}
}
impl AsyncPBSerialize for i64 {
	async fn serialize_async<W: AsyncWriteExt + Unpin + Send>(&self, w: &mut W) ->
		io::Result<()>
	{
		w.write_all(&self.to_be_bytes()).await
	}
}
impl AsyncPBDeserialize for f32 {
	async fn deserialize_async<R: AsyncReadExt + Unpin + Send>(r: &mut R) ->
		io::Result<Self>
	{
		let mut buf = [0; 4];
		r.read_exact(&mut buf).await?;
		Ok(Self::from_be_bytes(buf))
	}
}
impl AsyncPBSerialize for f32 {
	async fn serialize_async<W: AsyncWriteExt + Unpin + Send>(&self, w: &mut W) ->
		io::Result<()>
	{
		w.write_all(&self.to_be_bytes()).await
	}
}
impl AsyncPBDeserialize for f64 {
	async fn deserialize_async<R: AsyncReadExt + Unpin + Send>(r: &mut R) ->
		io::Result<Self>
	{
		let mut buf = [0; 8];
		r.read_exact(&mut buf).await?;
		Ok(Self::from_be_bytes(buf))
	}
}
impl AsyncPBSerialize for f64 {
	async fn serialize_async<W: AsyncWriteExt + Unpin + Send>(&self, w: &mut W) ->
		io::Result<()>
	{
		w.write_all(&self.to_be_bytes()).await
	}
}

impl<T: AsyncPBSerialize> AsyncPBSerialize for Vec<T> {
	async fn serialize_async<W: AsyncWriteExt + Unpin + Send>(&self, w: &mut W) ->
		io::Result<()>
	{
		let len = self.len() as u64;
		UInt(len).serialize_async(w).await?;
		for item in self {
			item.serialize_async(w).await?;
		}
		Ok(())
	}
}
impl<T: AsyncPBDeserialize> AsyncPBDeserialize for Vec<T> {
	async fn deserialize_async<R: AsyncReadExt + Unpin + Send>(r: &mut R) ->
		io::Result<Self>
	{
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

impl AsyncPBSerialize for Bytes<'_> {
	async fn serialize_async<W: AsyncWriteExt + Unpin + Send>(&self, w: &mut W) ->
		io::Result<()>
	{
		let len = self.0.len() as u64;
		UInt(len).serialize_async(w).await?;
		w.write_all(&self.0).await?;
		Ok(())
	}
}
impl AsyncPBDeserialize for Bytes<'_> {
	async fn deserialize_async<R: AsyncReadExt + Unpin + Send>(r: &mut R) ->
		io::Result<Self>
	{
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


impl AsyncPBSerialize for String {
	async fn serialize_async<W: AsyncWriteExt + Unpin + Send>(&self, w: &mut W) ->
		io::Result<()>
	{
		let len = self.len() as u64;
		UInt(len).serialize_async(w).await?;
		w.write_all(self.as_bytes()).await?;
		Ok(())
	}
}
impl AsyncPBDeserialize for String {
	async fn deserialize_async<R: AsyncReadExt + Unpin + Send>(r: &mut R) ->
		io::Result<Self>
	{
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
}

impl AsyncPBSerialize for Cow<'_, str> {
	async fn serialize_async<W: AsyncWriteExt + Unpin + Send>(&self, w: &mut W) ->
		io::Result<()>
	{
		self.to_string().serialize_async(w).await
	}
}
impl AsyncPBDeserialize for Cow<'_, str> {
	async fn deserialize_async<R: AsyncReadExt + Unpin + Send>(r: &mut R) ->
		io::Result<Self>
	{
		Ok(String::deserialize_async(r).await?.into())
	}
}


impl<K: AsyncPBType, V: AsyncPBType> AsyncPBSerialize for KeyPair<K, V> {
	async fn serialize_async<W: AsyncWriteExt + Unpin + Send>(&self, w: &mut W) ->
		io::Result<()>
	{
		self.key.serialize_async(w).await?;
		self.value.serialize_async(w).await?;
		Ok(())
	}
}
impl<K: AsyncPBType, V: AsyncPBType> AsyncPBDeserialize for KeyPair<K, V> {
	async fn deserialize_async<R: AsyncReadExt + Unpin + Send>(r: &mut R) ->
		io::Result<Self>
	{
		let field_key = K::deserialize_async(r).await?;
		let field_value = V::deserialize_async(r).await?;
		Ok(Self {
			key: field_key,
			value: field_value,
		})
	}
}

impl AsyncPBSerialize for bool {
	async fn serialize_async<W: AsyncWriteExt + Unpin + Send>(&self, w: &mut W) ->
		io::Result<()>
	{
		if *self {
			0u8.serialize_async(w).await?;
		} else {
			1u8.serialize_async(w).await?;
		}
		Ok(())
	}
}
impl AsyncPBDeserialize for bool {
	async fn deserialize_async<R: AsyncReadExt + Unpin + Send>(r: &mut R) ->
		io::Result<Self>
	{
		let discriminant = u8::deserialize_async(r).await?;
		Ok(match discriminant {
			0 => true,
			1 => false,
			_ => {
				Err(io::Error::other("Unknown enum discriminant; enum is not extensible"))?
			}
		})
	}
}

impl<T: AsyncPBType> AsyncPBSerialize for Option<T> {
	async fn serialize_async<W: AsyncWriteExt + Unpin + Send>(&self, w: &mut W) ->
		io::Result<()>
	{
		match self {
			None => {
				0u8.serialize_async(w).await?;
			}
			Some(value) => {
				1u8.serialize_async(w).await?;
				value.serialize_async(w).await?;
			}
		}
		Ok(())
	}
}
impl<T: AsyncPBType> AsyncPBDeserialize for Option<T> {
	async fn deserialize_async<R: AsyncReadExt + Unpin + Send>(r: &mut R) ->
		io::Result<Self>
	{
		let discriminant = u8::deserialize_async(r).await?;
		Ok(match discriminant {
			0 => {
				None
			}
			1 => {
				Some(T::deserialize_async(r).await?)
			}
			_ => {
				Err(io::Error::other("Unknown enum discriminant; enum is not extensible"))?
			}
		})
	}
}