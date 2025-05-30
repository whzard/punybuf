use std::io::{self, Error};
use tokio::io::{AsyncReadExt, AsyncWriteExt};

use crate::{const_unwrap, from_utf8_lossy_owned};

const MAX_BYTES_LENGTH: usize = const_unwrap!(usize::from_str_radix(env!("PUNYBUF_MAX_BYTES_LENGTH"), 10));
const MAX_ARRAY_LENGTH: usize = const_unwrap!(usize::from_str_radix(env!("PUNYBUF_MAX_ARRAY_LENGTH"), 10));

/// All Punybuf types implement this trait.
pub trait PBType: Sized + Send + Sync {
    const MIN_SIZE: usize;
    fn serialize<W: AsyncWriteExt + Unpin + Send>(&self, w: &mut W) -> impl std::future::Future<Output = io::Result<()>> + Send;
    fn deserialize<R: AsyncReadExt + Unpin + Send>(r: &mut R) -> impl std::future::Future<Output = io::Result<Self>> + Send;
}

/// An empty type, used as a return type for a command that doesn't need to return
/// anything, but needs to indicate that it's been recieved or that the requested
/// operation finished processing.
pub struct Done {}
impl PBType for Done {
    const MIN_SIZE: usize = 0;
    async fn serialize<W: AsyncWriteExt + Unpin + Send>(&self, _w: &mut W) -> io::Result<()> {
        Ok(())
    }
    async fn deserialize<R: AsyncReadExt + Unpin + Send>(_r: &mut R) -> io::Result<Self> {
        Ok(Self {})
    }
}

/// A variable-length integer. The greatest supported value is 1152921573328437375.
pub struct UInt(pub u64);
impl Into<u64> for UInt {
    fn into(self) -> u64 {
        self.0
    }
}
impl Into<usize> for UInt {
    fn into(self) -> usize {
        self.0 as usize
    }
}
impl From<u64> for UInt {
    fn from(value: u64) -> Self {
        Self(value as u64)
    }
}
impl From<usize> for UInt {
    fn from(value: usize) -> Self {
        Self(value as u64)
    }
}

impl PBType for UInt {
    const MIN_SIZE: usize = 1;
    async fn serialize<W: AsyncWriteExt + Unpin + Send>(&self, w: &mut W) -> io::Result<()> {
        let mut uint = self.0;
        if uint < 128 {
            w.write_all(&uint.to_be_bytes()[0..1]).await?;

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
                Err(io::Error::other("number too big (max 1152921573328437376)"))?;
            }
            Ok(())
    }
    async fn deserialize<R: AsyncReadExt + Unpin + Send>(r: &mut R) -> io::Result<Self> {
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
                Self(u64::from_be_bytes([buf[1], buf[0], 0, 0, 0, 0, 0, 0]) + 128)

            } else if first_byte & 0b001_00000 == 0 {
                // 110xxxxx
                buf[0] &= 0b000_11111;
                r.read_exact(&mut buf[1..3]).await?;
                Self(u64::from_be_bytes([buf[2], buf[1], buf[0], 0, 0, 0, 0, 0]) + 16512)

            } else if first_byte & 0b0001_0000 == 0 {
                // 1110xxxx
                buf[0] &= 0b0000_1111;
                r.read_exact(&mut buf[1..5]).await?;
                Self(u64::from_be_bytes([buf[4], buf[3], buf[2], buf[1], buf[0], 0, 0, 0]) + 2113664)

            } else {
                // 1111xxxx
                buf[0] &= 0b0000_1111;
                r.read_exact(&mut buf[1..8]).await?;
                Self(u64::from_be_bytes([buf[7], buf[6], buf[5], buf[4], buf[3], buf[2], buf[1], buf[0]]) + 68721590400)
            }
        )
    }
}

impl PBType for u8 {
    const MIN_SIZE: usize = 1;
    async fn deserialize<R: AsyncReadExt + Unpin + Send>(r: &mut R) -> io::Result<Self> {
        let mut buf = [0; 1];
        r.read_exact(&mut buf).await?;
        Ok(buf[0])
    }
    async fn serialize<W: AsyncWriteExt + Unpin + Send>(&self, w: &mut W) -> io::Result<()> {
        w.write_all(&[*self]).await
    }
}
impl PBType for u16 {
    const MIN_SIZE: usize = 2;
    async fn deserialize<R: AsyncReadExt + Unpin + Send>(r: &mut R) -> io::Result<Self> {
        let mut buf = [0; 2];
        r.read_exact(&mut buf).await?;
        Ok(Self::from_be_bytes(buf))
    }
    async fn serialize<W: AsyncWriteExt + Unpin + Send>(&self, w: &mut W) -> io::Result<()> {
        w.write_all(&self.to_be_bytes()).await
    }
}
impl PBType for u32 {
    const MIN_SIZE: usize = 4;
    async fn deserialize<R: AsyncReadExt + Unpin + Send>(r: &mut R) -> io::Result<Self> {
        let mut buf = [0; 4];
        r.read_exact(&mut buf).await?;
        Ok(Self::from_be_bytes(buf))
    }
    async fn serialize<W: AsyncWriteExt + Unpin + Send>(&self, w: &mut W) -> io::Result<()> {
        w.write_all(&self.to_be_bytes()).await
    }
}
impl PBType for u64 {
    const MIN_SIZE: usize = 8;
    async fn deserialize<R: AsyncReadExt + Unpin + Send>(r: &mut R) -> io::Result<Self> {
        let mut buf = [0; 8];
        r.read_exact(&mut buf).await?;
        Ok(Self::from_be_bytes(buf))
    }
    async fn serialize<W: AsyncWriteExt + Unpin + Send>(&self, w: &mut W) -> io::Result<()> {
        w.write_all(&self.to_be_bytes()).await
    }
}
impl PBType for i32 {
    const MIN_SIZE: usize = 4;
    async fn deserialize<R: AsyncReadExt + Unpin + Send>(r: &mut R) -> io::Result<Self> {
        let mut buf = [0; 4];
        r.read_exact(&mut buf).await?;
        Ok(Self::from_be_bytes(buf))
    }
    async fn serialize<W: AsyncWriteExt + Unpin + Send>(&self, w: &mut W) -> io::Result<()> {
        w.write_all(&self.to_be_bytes()).await
    }
}
impl PBType for i64 {
    const MIN_SIZE: usize = 8;
    async fn deserialize<R: AsyncReadExt + Unpin + Send>(r: &mut R) -> io::Result<Self> {
        let mut buf = [0; 8];
        r.read_exact(&mut buf).await?;
        Ok(Self::from_be_bytes(buf))
    }
    async fn serialize<W: AsyncWriteExt + Unpin + Send>(&self, w: &mut W) -> io::Result<()> {
        w.write_all(&self.to_be_bytes()).await
    }
}
impl PBType for f32 {
    const MIN_SIZE: usize = 4;
    async fn deserialize<R: AsyncReadExt + Unpin + Send>(r: &mut R) -> io::Result<Self> {
        let mut buf = [0; 4];
        r.read_exact(&mut buf).await?;
        Ok(Self::from_be_bytes(buf))
    }
    async fn serialize<W: AsyncWriteExt + Unpin + Send>(&self, w: &mut W) -> io::Result<()> {
        w.write_all(&self.to_be_bytes()).await
    }
}
impl PBType for f64 {
    const MIN_SIZE: usize = 8;
    async fn deserialize<R: AsyncReadExt + Unpin + Send>(r: &mut R) -> io::Result<Self> {
        let mut buf = [0; 8];
        r.read_exact(&mut buf).await?;
        Ok(Self::from_be_bytes(buf))
    }
    async fn serialize<W: AsyncWriteExt + Unpin + Send>(&self, w: &mut W) -> io::Result<()> {
        w.write_all(&self.to_be_bytes()).await
    }
}

impl<T: PBType> PBType for Vec<T> {
    const MIN_SIZE: usize = 1;
    async fn serialize<W: AsyncWriteExt + Unpin + Send>(&self, w: &mut W) -> io::Result<()> {
        let len = self.len() as u64;
        UInt(len).serialize(w).await?;
        for item in self {
            item.serialize(w).await?;
        }
        Ok(())
    }
    async fn deserialize<R: AsyncReadExt + Unpin + Send>(r: &mut R) -> io::Result<Self> {
        let len = UInt::deserialize(r).await?.into();
        if len > MAX_ARRAY_LENGTH {
            return Err(Error::other("Array length too large"));
        }
        let mut this = Vec::with_capacity(len);

        for _ in 0..len {
            this.push(T::deserialize(r).await?);
        }

        Ok(this)
    }
}

/// A convenience type wrapping a `Vec<u8>`, for more efficient (de)serialization.
pub struct Bytes(pub Vec<u8>);

impl PBType for Bytes {
    const MIN_SIZE: usize = 1;
    async fn serialize<W: AsyncWriteExt + Unpin + Send>(&self, w: &mut W) -> io::Result<()> {
        let len = self.0.len() as u64;
        UInt(len).serialize(w).await?;
        w.write_all(&self.0).await?;
        Ok(())
    }
    async fn deserialize<R: AsyncReadExt + Unpin + Send>(r: &mut R) -> io::Result<Self> {
        let len = UInt::deserialize(r).await?.into();
        if len > MAX_BYTES_LENGTH {
            return Err(Error::other("Bytes length too large"));
        }
        let mut this = Vec::with_capacity(len);
        let mut taken = r.take(len as u64);

        taken.read_to_end(&mut this).await?;

        Ok(Self(this))
    }
}

impl Into<Vec<u8>> for Bytes {
    fn into(self) -> Vec<u8> {
        self.0
    }
}


impl PBType for String {
    const MIN_SIZE: usize = 1;
    async fn deserialize<R: AsyncReadExt + Unpin + Send>(r: &mut R) -> io::Result<Self> {
        let len = UInt::deserialize(r).await?.into();
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

/// A trait that all commands implement.
pub trait PBCommand: Sized + Send + Sync {
    const MIN_SIZE: usize;
    type Error: PBType;
    type Return: PBType;

    fn id() -> u32;

    /// Convenience method to get the id of the command. Calls `id()` internally
    fn get_id(&self) -> u32 {
        Self::id()
    }

    /// Does **not** write the command ID.
    fn serialize_self<W: AsyncWriteExt + Unpin + Send>(&self, w: &mut W) -> impl std::future::Future<Output = io::Result<()>> + Send;

    /// Does **not** read the command ID.  
    /// If you need to read the command ID, use `Command::deserialize` from the generated file.
    fn deserialize<R: AsyncReadExt + Unpin + Send>(r: &mut R) -> impl std::future::Future<Output = io::Result<Self>> + Send;

    /// Writes both the command ID and the argument body
    fn serialize<W: AsyncWriteExt + Unpin + Send>(&self, w: &mut W) -> impl std::future::Future<Output = io::Result<()>> + Send {
        async {
            w.write_all(&Self::id().to_be_bytes()).await?;
            self.serialize_self(w).await
        }
    }
}