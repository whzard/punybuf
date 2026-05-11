use std::{collections::HashMap, fmt::{Debug, Display}, io::{self, Error, Read, Write}, ops::*};

pub use std::borrow::Cow;

mod const_macro;
const MAX_BYTES_LENGTH: usize = const_unwrap!(usize::from_str_radix(env!("PUNYBUF_MAX_BYTES_LENGTH"), 10));
const MAX_ARRAY_LENGTH: usize = const_unwrap!(usize::from_str_radix(env!("PUNYBUF_MAX_ARRAY_LENGTH"), 10));

#[cfg(feature = "tokio")]
pub mod tokio;

macro_rules! buffer_too_small {
	() => {
		io::Error::new(io::ErrorKind::UnexpectedEof, "buffer too small")
	}
}
/// All Punybuf types implement this trait.
pub trait PBType<'x> {
	fn attributes() -> &'static [(&'static str, Option<&'static str>)] { &[] }
	fn serialize<W: Write>(&self, w: &mut W) -> io::Result<()>;
	fn deserialize_stream<R: Read>(r: &mut R) -> io::Result<Self> where Self: Sized;
	fn deserialize<'a: 'x>(slice: &mut &'a [u8]) -> io::Result<Self> where Self: Sized {
		Self::deserialize_stream(slice)
	}
}

pub type Void = ();

impl<'x> PBType<'x> for Void {
	fn serialize<W: Write>(&self, _: &mut W) -> io::Result<()> {
		Ok(())
	}
	fn deserialize_stream<R: Read>(_: &mut R) -> io::Result<Self> where Self: Sized {
		Ok(())
	}
}

pub struct DuplicateKeysFound;
pub trait HashMapConvertible<K, V>: Sized {
	/// Converts the value to a `HashMap`, overriding duplicate keys.  
	/// Returns the resulting hashmap and a boolean indicating whether any duplicate keys were found
	fn to_map_allow_duplicates(self) -> (HashMap<K, V>, bool);

	/// Returns an error if there were any duplicate keys in the Map
	fn try_to_map(self) -> Result<HashMap<K, V>, DuplicateKeysFound> {
		let (map, duplicates_found) = self.to_map_allow_duplicates();
		if !duplicates_found {
			Ok(map)
		} else {
			Err(DuplicateKeysFound)
		}
	}
	fn from_map(map: std::collections::HashMap<K, V>) -> Self;
}

/// An empty type, used as a return type for a command that doesn't need to return
/// anything, but needs to indicate that it's been recieved or that the requested
/// operation finished processing.
#[derive(Debug)]
pub struct Done {}

impl<'x> PBType<'x> for Done {
	fn deserialize_stream<R: Read>(_r: &mut R) -> io::Result<Self> {
		Ok(Done {})
	}
	fn serialize<W: Write>(&self, _w: &mut W) -> io::Result<()> {
		Ok(())
	}
}

/// A variable-length integer. The greatest supported value is 1152921573328437375.
#[derive(Clone, Copy, PartialEq, Eq, PartialOrd, Ord)]
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
impl From<i32> for UInt {
	fn from(value: i32) -> Self {
		Self(value as u64)
	}
}

impl Debug for UInt {
	fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
		write!(f, "{}", self.0)
	}
}

impl Display for UInt {
	fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
		write!(f, "{}", self.0)
	}
}

impl BitOr<u64> for UInt {
	type Output = UInt;
	fn bitor(self, rhs: u64) -> Self::Output {
		Self(self.0 | rhs)
	}
}

impl BitOrAssign<u64> for UInt {
	fn bitor_assign(&mut self, rhs: u64) {
		self.0 |= rhs
	}
}

impl BitAnd<u64> for UInt {
	type Output = UInt;
	fn bitand(self, rhs: u64) -> Self::Output {
		Self(self.0 & rhs)
	}
}

impl BitAndAssign<u64> for UInt {
	fn bitand_assign(&mut self, rhs: u64) {
		self.0 &= rhs
	}
}

impl PartialEq<u64> for UInt {
	fn eq(&self, other: &u64) -> bool {
		&self.0 == other
	}
}

impl PartialOrd<u64> for UInt {
	fn partial_cmp(&self, other: &u64) -> Option<std::cmp::Ordering> {
		self.0.partial_cmp(other)
	}
}


impl<'x> PBType<'x> for UInt {
	fn serialize<W: Write>(&self, w: &mut W) -> io::Result<()> {
		let mut uint = self.0;
		if uint < 128 {
			w.write_all(&uint.to_be_bytes()[7..8])?;

			} else if uint < 16512 {
				uint -= 128;
				let bytes = &mut uint.to_be_bytes()[6..8];
				bytes[0] |= 0b10_000000;
				w.write_all(bytes)?;

			} else if uint < 2113664 {
				uint -= 16512;
				let bytes = &mut uint.to_be_bytes()[5..8];
				bytes[0] |= 0b110_00000;
				w.write_all(bytes)?;

			} else if uint < 68721590400 {
				uint -= 2113664;
				let bytes = &mut uint.to_be_bytes()[3..8];
				bytes[0] |= 0b1110_0000;
				w.write_all(bytes)?;

			} else if uint < 1152921573328437376 {
				uint -= 68721590400;
				let bytes = &mut uint.to_be_bytes()[0..8];
				bytes[0] |= 0b1111_0000;
				w.write_all(bytes)?;

			} else {
				Err(io::Error::other("number too big (max 1152921573328437375)"))?;
			}
			Ok(())
	}
	fn deserialize_stream<R: Read>(r: &mut R) -> io::Result<Self> {
		let mut first_byte = [0; 1];
		r.read_exact(&mut first_byte)?;
		
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
				r.read_exact(&mut buf[1..2])?;
				Self(u64::from_le_bytes([buf[1], buf[0], 0, 0, 0, 0, 0, 0]) + 128)

			} else if first_byte & 0b001_00000 == 0 {
				// 110xxxxx
				buf[0] &= 0b000_11111;
				r.read_exact(&mut buf[1..3])?;
				Self(u64::from_le_bytes([buf[2], buf[1], buf[0], 0, 0, 0, 0, 0]) + 16512)

			} else if first_byte & 0b0001_0000 == 0 {
				// 1110xxxx
				buf[0] &= 0b0000_1111;
				r.read_exact(&mut buf[1..5])?;
				Self(u64::from_le_bytes([buf[4], buf[3], buf[2], buf[1], buf[0], 0, 0, 0]) + 2113664)

			} else {
				// 1111xxxx
				buf[0] &= 0b0000_1111;
				r.read_exact(&mut buf[1..8])?;
				Self(u64::from_le_bytes([buf[7], buf[6], buf[5], buf[4], buf[3], buf[2], buf[1], buf[0]]) + 68721590400)
			}
		)
	}
	// there's no specialization for
	// deserialize() for numbers, since
	// the deserialize_stream version
	// is very fast anyways
}

impl<'x> PBType<'x> for u8 {
	fn deserialize_stream<R: Read>(r: &mut R) -> io::Result<Self> {
		let mut buf = [0; 1];
		r.read_exact(&mut buf)?;
		Ok(buf[0])
	}
	fn serialize<W: Write>(&self, w: &mut W) -> io::Result<()> {
		w.write_all(&[*self])
	}
}
impl<'x> PBType<'x> for u16 {
	fn deserialize_stream<R: Read>(r: &mut R) -> io::Result<Self> {
		let mut buf = [0; 2];
		r.read_exact(&mut buf)?;
		Ok(Self::from_be_bytes(buf))
	}
	fn serialize<W: Write>(&self, w: &mut W) -> io::Result<()> {
		w.write_all(&self.to_be_bytes())
	}
}
impl<'x> PBType<'x> for u32 {
	fn deserialize_stream<R: Read>(r: &mut R) -> io::Result<Self> {
		let mut buf = [0; 4];
		r.read_exact(&mut buf)?;
		Ok(Self::from_be_bytes(buf))
	}
	fn serialize<W: Write>(&self, w: &mut W) -> io::Result<()> {
		w.write_all(&self.to_be_bytes())
	}
}
impl<'x> PBType<'x> for u64 {
	fn deserialize_stream<R: Read>(r: &mut R) -> io::Result<Self> {
		let mut buf = [0; 8];
		r.read_exact(&mut buf)?;
		Ok(Self::from_be_bytes(buf))
	}
	fn serialize<W: Write>(&self, w: &mut W) -> io::Result<()> {
		w.write_all(&self.to_be_bytes())
	}
}
impl<'x> PBType<'x> for i32 {
	fn deserialize_stream<R: Read>(r: &mut R) -> io::Result<Self> {
		let mut buf = [0; 4];
		r.read_exact(&mut buf)?;
		Ok(Self::from_be_bytes(buf))
	}
	fn serialize<W: Write>(&self, w: &mut W) -> io::Result<()> {
		w.write_all(&self.to_be_bytes())
	}
}
impl<'x> PBType<'x> for i64 {
	fn deserialize_stream<R: Read>(r: &mut R) -> io::Result<Self> {
		let mut buf = [0; 8];
		r.read_exact(&mut buf)?;
		Ok(Self::from_be_bytes(buf))
	}
	fn serialize<W: Write>(&self, w: &mut W) -> io::Result<()> {
		w.write_all(&self.to_be_bytes())
	}
}
impl<'x> PBType<'x> for f32 {
	fn deserialize_stream<R: Read>(r: &mut R) -> io::Result<Self> {
		let mut buf = [0; 4];
		r.read_exact(&mut buf)?;
		Ok(Self::from_be_bytes(buf))
	}
	fn serialize<W: Write>(&self, w: &mut W) -> io::Result<()> {
		w.write_all(&self.to_be_bytes())
	}
}
impl<'x> PBType<'x> for f64 {
	fn deserialize_stream<R: Read>(r: &mut R) -> io::Result<Self> {
		let mut buf = [0; 8];
		r.read_exact(&mut buf)?;
		Ok(Self::from_be_bytes(buf))
	}
	fn serialize<W: Write>(&self, w: &mut W) -> io::Result<()> {
		w.write_all(&self.to_be_bytes())
	}
}

impl<'x, T: PBType<'x>> PBType<'x> for Vec<T> {
	fn serialize<W: Write>(&self, w: &mut W) -> io::Result<()> {
		let len = self.len() as u64;
		UInt(len).serialize(w)?;
		for item in self {
			item.serialize(w)?;
		}
		Ok(())
	}
	fn deserialize_stream<R: Read>(r: &mut R) -> io::Result<Self> {
		let len = UInt::deserialize_stream(r)?.into();
		if len > MAX_ARRAY_LENGTH {
			return Err(Error::other("Array length too large"));
		}
		let mut this = Vec::with_capacity(len);

		for _ in 0..len {
			this.push(T::deserialize_stream(r)?);
		}

		Ok(this)
	}
	fn deserialize<'a: 'x>(slice: &mut &'a [u8]) -> io::Result<Self> where Self: Sized {
		let len = UInt::deserialize(slice)?.into();
		if len > MAX_ARRAY_LENGTH {
			return Err(Error::other("Array length too large"));
		}
		let mut this = Vec::with_capacity(len);

		for _ in 0..len {
			this.push(T::deserialize(slice)?);
		}

		Ok(this)
	}
}

/// A convenience type wrapping a `Vec<u8>`, for more efficient (de)serialization.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Bytes<'a>(Cow<'a, [u8]>);

impl<'x> PBType<'x> for Bytes<'x> {
	fn serialize<W: Write>(&self, w: &mut W) -> io::Result<()> {
		let len = self.0.len() as u64;
		UInt(len).serialize(w)?;
		w.write_all(&self.0)?;
		Ok(())
	}
	fn deserialize_stream<R: Read>(r: &mut R) -> io::Result<Self> {
		let len = UInt::deserialize_stream(r)?.into();
		if len > MAX_BYTES_LENGTH {
			return Err(Error::other("Bytes length too large"));
		}
		let mut this = Vec::with_capacity(len);
		let mut taken = r.take(len as u64);

		taken.read_to_end(&mut this)?;

		Ok(Self(this.into()))
	}
	fn deserialize<'a: 'x>(slice: &mut &'a [u8]) -> io::Result<Self> {
		let len = UInt::deserialize(slice)?.into();
		if len > MAX_BYTES_LENGTH {
			return Err(Error::other("Bytes length too large"));
		}
		if slice.len() < len {
			Err(buffer_too_small!())?;
		}
		let result = &slice.get(0..len).unwrap();
		let new_slice = &slice.get(len..slice.len()).unwrap();
		*slice = new_slice;
		Ok(Self(Cow::Borrowed(result)))
	}
}

impl Into<Vec<u8>> for Bytes<'_> {
	fn into(self) -> Vec<u8> {
		self.0.into()
	}
}

impl<'a> Into<Cow<'a, [u8]>> for Bytes<'a> {
	fn into(self) -> Cow<'a, [u8]> {
		self.0
	}
}

impl From<Vec<u8>> for Bytes<'_> {
	fn from(value: Vec<u8>) -> Self {
		Self(value.into())
	}
}

impl<'a> From<Cow<'a, [u8]>> for Bytes<'a> {
	fn from(value: Cow<'a, [u8]>) -> Self {
		Self(value)
	}
}

pub(crate) fn from_utf8_lossy_owned(v: Vec<u8>) -> String {
	if let Cow::Owned(string) = String::from_utf8_lossy(&v) {
		string
	} else {
		// SAFETY: `String::from_utf8_lossy`'s contract ensures that if
		// it returns a `Cow::Borrowed`, it is a valid UTF-8 string.
		// Otherwise, it returns a new allocation of an owned `String`, with
		// replacement characters for invalid sequences, which is returned
		// above.
		unsafe { String::from_utf8_unchecked(v) }
	}
}


impl<'x> PBType<'x> for Cow<'x, str> {
	fn serialize<W: Write>(&self, w: &mut W) -> io::Result<()> {
		let len = self.len() as u64;
		UInt(len).serialize(w)?;
		w.write_all(self.as_bytes())?;
		Ok(())
	}

	fn deserialize_stream<R: Read>(r: &mut R) -> io::Result<Self> where Self: Sized {
		Ok(String::deserialize_stream(r)?.into())
	}
	fn deserialize<'a: 'x>(slice: &mut &'a [u8]) -> io::Result<Self> {
		let len = UInt::deserialize(slice)?.into();
		if len > MAX_BYTES_LENGTH {
			return Err(Error::other("Bytes length too large"));
		}
		if slice.len() < len {
			Err(buffer_too_small!())?;
		}
		let result = &slice.get(0..len).unwrap();
		let new_slice = &slice.get(len..slice.len()).unwrap();
		*slice = new_slice;
		Ok(String::from_utf8_lossy(result))
	}
}

// Legacy code
impl<'x> PBType<'x> for String {
	fn deserialize_stream<R: Read>(r: &mut R) -> io::Result<Self> {
		let len = UInt::deserialize_stream(r)?.into();
		if len > MAX_BYTES_LENGTH {
			return Err(Error::other("String length too large"));
		}

		let mut this = Vec::with_capacity(len);
		let mut taken = r.take(len as u64);

		taken.read_to_end(&mut this)?;

		Ok(from_utf8_lossy_owned(this))
	}
	fn serialize<W: Write>(&self, w: &mut W) -> io::Result<()> {
		let len = self.len() as u64;
		UInt(len).serialize(w)?;
		w.write_all(self.as_bytes())?;
		Ok(())
	}
}

/// A trait that all individual commands implement. The enum of all commands *does not* implement this trait.
pub trait PBCommandExt<'x> {
	type Error<'a>: PBType<'a>;
	type Return<'a>: PBType<'a>;

	/// The ID of the command.
	const ID: u32;
	/// Whether the `Return` type is `Void`.
	const IS_VOID: bool = false;

	const ATTRIBUTES: &'static [(&'static str, Option<&'static str>)] = &[];
	const REQUIRED_CAPABILITY: Option<&'static str> = None;

	fn deserialize_return_stream<R: Read>(&self, r: &mut R) -> io::Result<Self::Return<'static>> {
		Self::Return::deserialize_stream(r)
	}
	fn deserialize_return<'a>(slice: &mut &'a[u8]) -> io::Result<Self::Return<'a>> {
		Self::Return::deserialize(slice)
	}
	fn deserialize_error_stream<R: Read>(&self, r: &mut R) -> io::Result<Self::Error<'static>> {
		Self::Error::deserialize_stream(r)
	}
	fn deserialize_error<'a>(slice: &mut &'a[u8]) -> io::Result<Self::Error<'a>> {
		Self::Error::deserialize(slice)
	}

	/// Does **not** read the command ID.  
	/// If you need to read the command ID, use `CommandID::deserialize`
	fn deserialize_stream<R: Read>(r: &mut R) -> io::Result<Self> where Self: Sized;
	fn deserialize<'a: 'x>(slice: &mut &'a[u8]) -> io::Result<Self> where Self: Sized {
		Self::deserialize_stream(slice)
	}
}

/// A trait that all commands implement. The enum of all commands also implements this trait.
pub trait PBCommand {
	fn id(&self) -> u32;

	/// Whether the `Return` type is `Void`
	fn is_void(&self) -> bool { false }

	fn attributes(&self) -> &'static [(&'static str, Option<&'static str>)] { &[] }
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

// TODO: write more tests
#[cfg(test)]
mod libtest {
	use std::time::{Duration, SystemTime};

	const TEST_UINTS: &[u64] = &[
		0, 32, 64, 127, 128, 129,
		16511, 16512, 16513,
		2113663, 2113664, 2113665,
		68721590399, 68721590400, 68721590401,
		1152921573328437375
	];

	#[test]
	fn uint_correct() {
		use crate::{PBType, UInt};
		for n in TEST_UINTS {
			let mut v = vec![];
			UInt(*n).serialize(&mut v).unwrap();
			let r = &mut &v[..];
			let same = UInt::deserialize_stream(r).unwrap();
			assert_eq!(same.0, *n);
			assert_eq!(*r, &[]);
		}
		for n in TEST_UINTS {
			let mut v = vec![];
			UInt(*n).serialize(&mut v).unwrap();
			let r = &mut &v[..];
			let same = UInt::deserialize(r).unwrap();
			assert_eq!(same.0, *n);
			assert_eq!(*r, &[]);
		}
	}
	
	#[tokio::test]
	async fn async_uint_correct() {
		use crate::tokio::{PBType, UInt};
		for n in TEST_UINTS {
			let mut v = vec![];
			UInt(*n).serialize(&mut v).await.unwrap();
			let same = UInt::deserialize_stream(&mut &v[..]).await.unwrap();
			assert_eq!(same.0, *n);
		}
	}

	const TEST_STRINGS: &[&str] = &[
		"",
		"some_string",
		"кириллица",
		"emoji: \u{1F431}",
	];

	#[test]
	fn strings() {
		use std::borrow::Cow;
		use crate::{PBType};
		let mut v = vec![];
		for s in TEST_STRINGS {
			Cow::Borrowed(*s).serialize(&mut v).unwrap();
		}
		let mut slice: &[u8] = &v;
		for s in TEST_STRINGS {
			let res: Cow<'_, str> = Cow::deserialize(&mut slice).unwrap();
			assert_eq!(res.as_ref(), *s);
		}
		let mut slice: &[u8] = &v;
		for s in TEST_STRINGS {
			let res: Cow<'_, str> = Cow::deserialize_stream(&mut slice).unwrap();
			assert_eq!(res.as_ref(), *s);
		}
	}
}