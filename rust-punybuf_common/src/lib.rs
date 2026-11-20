#![doc = include_str!("../README.md")]

use std::{
	collections::HashMap,
	fmt::{Debug, Display},
	io::{self, Error, Read, Write},
	ops::*
};

use std::borrow::Cow;

mod const_macro;
use const_macro::const_expect;
const MAX_BYTES_LENGTH: usize = const_expect!(
	usize::from_str_radix(env!("PUNYBUF_MAX_BYTES_LENGTH"), 10),
	"invalid value of env variable PUNYBUF_MAX_BYTES_LENGTH"
);
const MAX_ARRAY_LENGTH: usize = const_expect!(
	usize::from_str_radix(env!("PUNYBUF_MAX_ARRAY_LENGTH"), 10),
	"invalid value of env variable PUNYBUF_MAX_BYTES_LENGTH"
);

#[cfg(feature = "tokio")]
pub mod tokio;

pub mod commands;

/// Contains all items that the generated definitions must import.
///
/// # Stability
/// It's not advisable to import this in user code, since items may
/// be added without incrementing the major version, potentially causing
/// conflicts.
pub mod prelude {
	pub use crate::{
		PBType, PBSerialize, PBDeserialize, HashMapConvertible,
		UInt, Bytes, DuplicateKeysFound,
		Done, Map, KeyPair, Void,
		commands::{
			PBCommand, PBCommandDeserialize, PBCommandSerialize,
			PBProtocol, ProtocolCommands, ProtocolErrors, ProtocolReturns,
			MatchID
		}
	};
}

/// The version is automatically checked in the generated code by importing
/// these symbols from the `punybuf_common` crate and checking their existence.
// TODO: lmao this is so scuffed
pub mod supported_versions {
	/// The major version of this crate (`PRE` means we're not yet 1.0)
	pub const PRE_MAJOR_13: usize = 13;
	/// A supported minor version of this crate
	pub const MINOR_0: usize = 0;
	/// A supported minor version of this crate
	pub const MINOR_1: usize = 1;
}

macro_rules! buffer_too_small {
	() => {
		io::Error::new(io::ErrorKind::UnexpectedEof, "buffer too small")
	}
}
macro_rules! eof {
	() => {
		io::Error::new(io::ErrorKind::UnexpectedEof, "unexpected EOF")
	}
}
/// Interface trait to enable serialization of Punybuf types.
pub trait PBSerialize {
	fn attributes(&self) -> &'static [(&'static str, Option<&'static str>)] { &[] }
	fn serialize<W: Write>(&self, w: &mut W) -> io::Result<()>;
}
/// Static trait to enable deserialization of Punybuf types.
pub trait PBDeserialize<'x> {
	const ATTRIBUTES: &'static [(&'static str, Option<&'static str>)] = &[];
	fn deserialize_owned<R: Read>(r: &mut R) -> io::Result<Self>
		where Self: Sized;
	fn deserialize<'a: 'x>(slice: &mut &'a [u8]) -> io::Result<Self>
		where Self: Sized
	{
		Self::deserialize_owned(slice)
	}
}

/// Common trait that all types implement.
///
/// This trait has a blanket implementation for all types implementing
/// [`PBSerialize`] and [`PBDeserialize`]
pub trait PBType<'x>: PBSerialize + PBDeserialize<'x> {}

impl<'x, T> PBType<'x> for T where T: PBSerialize + PBDeserialize<'x> {}

impl<'x, T> PBSerialize for Box<T> where T: PBSerialize {
	fn serialize<W: Write>(&self, w: &mut W) -> io::Result<()> {
		self.deref().serialize(w)
	}
}
impl<'x, T> PBDeserialize<'x> for Box<T> where T: PBDeserialize<'x> {
	fn deserialize_owned<R: Read>(r: &mut R) -> io::Result<Self>
		where Self: Sized
	{
		T::deserialize_owned(r).map(|x| Box::new(x))
	}
	fn deserialize<'a: 'x>(slice: &mut &'a [u8]) -> io::Result<Self>
		where Self: Sized
	{
		T::deserialize(slice).map(|x| Box::new(x))
	}
}

pub type Void = ();

impl<'x> PBSerialize for Void {
	fn serialize<W: Write>(&self, _: &mut W) -> io::Result<()> {
		Ok(())
	}
}
impl<'x> PBDeserialize<'x> for Void {
	fn deserialize_owned<R: Read>(_: &mut R) -> io::Result<Self>
		where Self: Sized
	{
		Ok(())
	}
}

pub struct DuplicateKeysFound;
pub trait HashMapConvertible<K, V>: Sized {
	/// Convert the value to a `HashMap`, overriding duplicate keys.
	///
	/// Returns the resulting `HashMap` and a boolean indicating whether any duplicate keys were found
	fn to_map_allow_duplicates(self) -> (HashMap<K, V>, bool);

	/// Convert the value to a `HashMap`
	///
	/// Returns an error if there were any duplicate keys in the Map
	fn try_to_map(self) -> Result<HashMap<K, V>, DuplicateKeysFound> {
		let (map, duplicates_found) = self.to_map_allow_duplicates();
		if !duplicates_found {
			Ok(map)
		} else {
			Err(DuplicateKeysFound)
		}
	}

	/// Build this type from a `HashMap`
	// TODO:1.0: update this API to use `impl Iterator<Item = (K, V)>`
	fn from_map(map: HashMap<K, V>) -> Self;
}

/// An empty type, used as a return type for a command that doesn't need to return
/// anything, but needs to indicate that it's been recieved or that the requested
/// operation finished processing.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Done {}

impl<'x> PBSerialize for Done {
	fn serialize<W: Write>(&self, _w: &mut W) -> io::Result<()> {
		Ok(())
	}
}
impl<'x> PBDeserialize<'x> for Done {
	fn deserialize_owned<R: Read>(_r: &mut R) -> io::Result<Self> {
		Ok(Done {})
	}
}

/// A variable-length integer. The greatest supported value is 1152921573328437375.
// TODO: make this constructor private and panic when the value is greater than the
// maximum.
// TODO: run benchmarks to see if this UInt encoding is really all that efficient
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


impl<'x> PBSerialize for UInt {
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
}
impl<'x> PBDeserialize<'x> for UInt {
	fn deserialize_owned<R: Read>(r: &mut R) -> io::Result<Self> {
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
	// the deserialize_owned version
	// is very fast anyways
}

impl<'x> PBSerialize for u8 {
	fn serialize<W: Write>(&self, w: &mut W) -> io::Result<()> {
		w.write_all(&[*self])
	}
}
impl<'x> PBDeserialize<'x> for u8 {
	fn deserialize_owned<R: Read>(r: &mut R) -> io::Result<Self> {
		let mut buf = [0; 1];
		r.read_exact(&mut buf)?;
		Ok(buf[0])
	}
}
impl<'x> PBSerialize for u16 {
	fn serialize<W: Write>(&self, w: &mut W) -> io::Result<()> {
		w.write_all(&self.to_be_bytes())
	}
}
impl<'x> PBDeserialize<'x> for u16 {
	fn deserialize_owned<R: Read>(r: &mut R) -> io::Result<Self> {
		let mut buf = [0; 2];
		r.read_exact(&mut buf)?;
		Ok(Self::from_be_bytes(buf))
	}
}
impl<'x> PBSerialize for u32 {
	fn serialize<W: Write>(&self, w: &mut W) -> io::Result<()> {
		w.write_all(&self.to_be_bytes())
	}
}
impl<'x> PBDeserialize<'x> for u32 {
	fn deserialize_owned<R: Read>(r: &mut R) -> io::Result<Self> {
		let mut buf = [0; 4];
		r.read_exact(&mut buf)?;
		Ok(Self::from_be_bytes(buf))
	}
}
impl<'x> PBSerialize for u64 {
	fn serialize<W: Write>(&self, w: &mut W) -> io::Result<()> {
		w.write_all(&self.to_be_bytes())
	}
}
impl<'x> PBDeserialize<'x> for u64 {
	fn deserialize_owned<R: Read>(r: &mut R) -> io::Result<Self> {
		let mut buf = [0; 8];
		r.read_exact(&mut buf)?;
		Ok(Self::from_be_bytes(buf))
	}
}
impl<'x> PBSerialize for i32 {
	fn serialize<W: Write>(&self, w: &mut W) -> io::Result<()> {
		w.write_all(&self.to_be_bytes())
	}
}
impl<'x> PBDeserialize<'x> for i32 {
	fn deserialize_owned<R: Read>(r: &mut R) -> io::Result<Self> {
		let mut buf = [0; 4];
		r.read_exact(&mut buf)?;
		Ok(Self::from_be_bytes(buf))
	}
}
impl<'x> PBSerialize for i64 {
	fn serialize<W: Write>(&self, w: &mut W) -> io::Result<()> {
		w.write_all(&self.to_be_bytes())
	}
}
impl<'x> PBDeserialize<'x> for i64 {
	fn deserialize_owned<R: Read>(r: &mut R) -> io::Result<Self> {
		let mut buf = [0; 8];
		r.read_exact(&mut buf)?;
		Ok(Self::from_be_bytes(buf))
	}
}
impl<'x> PBSerialize for f32 {
	fn serialize<W: Write>(&self, w: &mut W) -> io::Result<()> {
		w.write_all(&self.to_be_bytes())
	}
}
impl<'x> PBDeserialize<'x> for f32 {
	fn deserialize_owned<R: Read>(r: &mut R) -> io::Result<Self> {
		let mut buf = [0; 4];
		r.read_exact(&mut buf)?;
		Ok(Self::from_be_bytes(buf))
	}
}
impl<'x> PBSerialize for f64 {
	fn serialize<W: Write>(&self, w: &mut W) -> io::Result<()> {
		w.write_all(&self.to_be_bytes())
	}
}
impl<'x> PBDeserialize<'x> for f64 {
	fn deserialize_owned<R: Read>(r: &mut R) -> io::Result<Self> {
		let mut buf = [0; 8];
		r.read_exact(&mut buf)?;
		Ok(Self::from_be_bytes(buf))
	}
}

impl<'x, T: PBSerialize> PBSerialize for Vec<T> {
	fn serialize<W: Write>(&self, w: &mut W) -> io::Result<()> {
		let len = self.len() as u64;
		UInt(len).serialize(w)?;
		for item in self {
			item.serialize(w)?;
		}
		Ok(())
	}
}
impl<'x, T: PBDeserialize<'x>> PBDeserialize<'x> for Vec<T> {
	fn deserialize_owned<R: Read>(r: &mut R) -> io::Result<Self> {
		let len = UInt::deserialize_owned(r)?.into();
		if len > MAX_ARRAY_LENGTH {
			return Err(Error::other("Array length too large"));
		}
		let mut this = Vec::with_capacity(len);

		for _ in 0..len {
			this.push(T::deserialize_owned(r)?);
		}

		Ok(this)
	}
	fn deserialize<'a: 'x>(slice: &mut &'a [u8]) -> io::Result<Self>
		where Self: Sized
	{
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
pub struct Bytes<'a>(pub Cow<'a, [u8]>);

impl<'x> PBSerialize for Bytes<'x> {
	fn serialize<W: Write>(&self, w: &mut W) -> io::Result<()> {
		let len = self.0.len() as u64;
		UInt(len).serialize(w)?;
		w.write_all(&self.0)?;
		Ok(())
	}
}
impl<'x> PBDeserialize<'x> for Bytes<'x> {
	fn deserialize_owned<R: Read>(r: &mut R) -> io::Result<Self> {
		let len = UInt::deserialize_owned(r)?.into();
		if len > MAX_BYTES_LENGTH {
			return Err(Error::other("Bytes length too large"));
		}
		let mut this = Vec::with_capacity(len);
		let mut taken = r.take(len as u64);

		taken.read_to_end(&mut this)?;
		if this.len() != len {
			eof!();
		}

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


impl<'x> PBSerialize for Cow<'x, str> {
	fn serialize<W: Write>(&self, w: &mut W) -> io::Result<()> {
		let len = self.len() as u64;
		UInt(len).serialize(w)?;
		w.write_all(self.as_bytes())?;
		Ok(())
	}
}
impl<'x> PBDeserialize<'x> for Cow<'x, str> {
	fn deserialize_owned<R: Read>(r: &mut R) -> io::Result<Self>
		where Self: Sized
	{
		Ok(String::deserialize_owned(r)?.into())
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
		Ok(str::from_utf8(result).map_err(|e|
			io::Error::new(io::ErrorKind::InvalidData, e)
		)?.into())
	}
}

impl<'x> PBSerialize for String {
	fn serialize<W: Write>(&self, w: &mut W) -> io::Result<()> {
		let len = self.len() as u64;
		UInt(len).serialize(w)?;
		w.write_all(self.as_bytes())?;
		Ok(())
	}
}
impl<'x> PBDeserialize<'x> for String {
	fn deserialize_owned<R: Read>(r: &mut R) -> io::Result<Self> {
		let len = UInt::deserialize_owned(r)?.into();
		if len > MAX_BYTES_LENGTH {
			return Err(Error::other("String length too large"));
		}

		let mut this = Vec::with_capacity(len);
		let mut taken = r.take(len as u64);

		taken.read_to_end(&mut this)?;
		if this.len() != len {
			eof!();
		}

		Ok(String::from_utf8(this).map_err(|e|
			io::Error::new(io::ErrorKind::InvalidData, e)
		)?)
	}
}

impl<'x, K, V> HashMapConvertible<K, V> for Map<K, V>
	where K: PBType<'x> + std::hash::Hash + Eq, V: PBType<'x>
{
	fn to_map_allow_duplicates(self) -> (HashMap<K, V>, bool) {
		let mut hm = HashMap::new();
		let mut duplicates = false;
		for pair in self {
			if hm.insert(pair.key, pair.value).is_some() {
				duplicates = true;
			}
		}
		(hm, duplicates)
	}
	fn from_map(map: HashMap<K, V>) -> Self {
		let mut this = Self::new();
		for (key, value) in map.into_iter() {
			this.push(KeyPair { key, value });
		}
		this
	}
}
/// A map type. This isn't marked `@builtin`, but implementations may, for their
/// own convenience, allow to convert this type to their own `HashMap`
/// implementation. This conversion may fail, as this type enforces no rules
/// on the uniquness of the keys.
/// 
/// In the case that one of the keys is not unique, the implementation SHOULD NOT
/// reject a frame or fail the deserialization completely, but should react to this error
/// in some other way, like telling the user or throwing a more specific error.
pub type Map<K, V> = Vec<KeyPair<K, V>>;
#[derive(Debug, Clone, PartialEq)]
pub struct KeyPair<K, V> {
	pub key: K,
	pub value: V,
}
impl<'x, K: PBType<'x>, V: PBType<'x>> PBSerialize for KeyPair<K, V> {
	fn attributes(&self) -> &'static [(&'static str, Option<&'static str>)]
		{ Self::ATTRIBUTES }
	fn serialize<W: io::Write>(&self, w: &mut W) -> io::Result<()> {
		self.key.serialize(w)?;
		self.value.serialize(w)?;
		Ok(())
	}
}
impl<'x, K: PBType<'x>, V: PBType<'x>> PBDeserialize<'x> for KeyPair<K, V> {
	const ATTRIBUTES: &'static [(&'static str, Option<&'static str>)] = &[
		("@sealed", None),
	];
	fn deserialize_owned<R: io::Read>(r: &mut R) -> io::Result<Self> {
		let field_key = K::deserialize_owned(r)?;
		let field_value = V::deserialize_owned(r)?;
		Ok(Self {
			key: field_key,
			value: field_value,
		})
	}
	fn deserialize<'a: 'x>(r: &mut &'a [u8]) -> io::Result<Self> {
		let field_key = K::deserialize(r)?;
		let field_value = V::deserialize(r)?;
		Ok(Self {
			key: field_key,
			value: field_value,
		})
	}
}

impl<'x> PBSerialize for bool {
	fn serialize<W: io::Write>(&self, w: &mut W) -> io::Result<()> {
		if *self {
			0u8.serialize(w)?;
		} else {
			1u8.serialize(w)?;
		}
		Ok(())
	}
}
impl<'x> PBDeserialize<'x> for bool {
	fn deserialize_owned<R: io::Read>(r: &mut R) -> io::Result<Self> {
		let discriminant = u8::deserialize_owned(r)?;
		Ok(match discriminant {
			0 => true,
			1 => false,
			_ => {
				Err(
					io::Error::other("Unknown enum discriminant; enum is not extensible")
				)?
			}
		})
	}
	fn deserialize<'a: 'x>(r: &mut &'a [u8]) -> io::Result<Self> {
		let discriminant = u8::deserialize(r)?;
		Ok(match discriminant {
			0 => true,
			1 => false,
			_ => {
				Err(
					io::Error::other("Unknown enum discriminant; enum is not extensible")
				)?
			}
		})
	}
}
impl<'x, T: PBType<'x>> PBSerialize for Option<T> {
	fn serialize<W: io::Write>(&self, w: &mut W) -> io::Result<()> {
		match self {
			None => {
				0u8.serialize(w)?;
			}
			Some(value) => {
				1u8.serialize(w)?;
				value.serialize(w)?;
			}
		}
		Ok(())
	}
}
impl<'x, T: PBType<'x>> PBDeserialize<'x> for Option<T> {
	fn deserialize_owned<R: io::Read>(r: &mut R) -> io::Result<Self> {
		let discriminant = u8::deserialize_owned(r)?;
		Ok(match discriminant {
			0 => {
				None
			}
			1 => {
				Some(T::deserialize_owned(r)?)
			}
			_ => {
				Err(
					io::Error::other("Unknown enum discriminant; enum is not extensible")
				)?
			}
		})
	}
	fn deserialize<'a: 'x>(r: &mut &'a [u8]) -> io::Result<Self> {
		let discriminant = u8::deserialize(r)?;
		Ok(match discriminant {
			0 => {
				None
			}
			1 => {
				Some(T::deserialize(r)?)
			}
			_ => {
				Err(
					io::Error::other("Unknown enum discriminant; enum is not extensible")
				)?
			}
		})
	}
}

// TODO: write more tests
#[cfg(test)]
mod libtest {
	use crate::{
		PBSerialize, PBDeserialize, UInt, KeyPair,
		tokio::{AsyncPBDeserialize, AsyncPBSerialize}
	};
	use std::borrow::Cow;

	const TEST_UINTS: &[u64] = &[
		0, 32, 64, 127, 128, 129,
		16511, 16512, 16513,
		2113663, 2113664, 2113665,
		68721590399, 68721590400, 68721590401,
		1152921573328437375
	];

	#[test]
	fn uint_correct() {
		for n in TEST_UINTS {
			let mut v = vec![];
			UInt(*n).serialize(&mut v).unwrap();
			let r = &mut &v[..];
			let same = UInt::deserialize_owned(r).unwrap();
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
		for n in TEST_UINTS {
			let mut v = vec![];
			UInt(*n).serialize_async(&mut v).await.unwrap();
			let r = &mut &v[..];
			let same = UInt::deserialize_async(r).await.unwrap();
			assert_eq!(same.0, *n);
			assert_eq!(*r, &[]);
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
			let res: Cow<'_, str> = Cow::deserialize_owned(&mut slice).unwrap();
			assert_eq!(res.as_ref(), *s);
		}
	}

	#[test]
	fn streaming_key_pair() {
		let mut vec = Vec::<u8>::new();
		let thing = KeyPair { key: Cow::from("Hello, world!"), value: Cow::from("Hello, world!"), };
		thing.serialize(&mut vec).unwrap();
		let r = &mut &vec[..];
		let other = KeyPair::<Cow<'_, str>, Cow<'_, str>>::deserialize_owned(r).unwrap();
		assert_eq!(thing, other, "values don't match");
		assert_eq!(*r, &[], "cleanup");
	}
	#[test]
	fn in_place_key_pair() {
		let mut vec = Vec::<u8>::new();
		let thing = KeyPair { key: Cow::from("Hello, world!"), value: Cow::from("Hello, world!"), };
		thing.serialize(&mut vec).unwrap();
		let r = &mut &vec[..];
		let other = KeyPair::<Cow<'_, str>, Cow<'_, str>>::deserialize(r).unwrap();
		assert_eq!(thing, other, "values don't match");
		assert_eq!(*r, &[], "cleanup");
	}
	#[tokio::test]
	async fn async_key_pair() {
		let mut vec = Vec::<u8>::new();
		let thing = KeyPair { key: Cow::from("Hello, world!"), value: Cow::from("Hello, world"), };
		thing.serialize_async(&mut vec).await.unwrap();
		let r = &mut &vec[..];
		let other = KeyPair::<Cow<'_, str>, Cow<'_, str>>::deserialize_async(r).await.unwrap();
		assert_eq!(thing, other, "values don't match");
		assert_eq!(*r, &[], "cleanup");
	}
	#[test]
	fn streaming_boolean() {
		let mut vec = Vec::<u8>::new();
		let thing = true;
		thing.serialize(&mut vec).unwrap();
		let r = &mut &vec[..];
		let other = bool::deserialize_owned(r).unwrap();
		assert_eq!(thing, other, "values don't match");
		assert_eq!(*r, &[], "cleanup");
	}
	#[test]
	fn in_place_boolean() {
		let mut vec = Vec::<u8>::new();
		let thing = false;
		thing.serialize(&mut vec).unwrap();
		let r = &mut &vec[..];
		let other = bool::deserialize(r).unwrap();
		assert_eq!(thing, other, "values don't match");
		assert_eq!(*r, &[], "cleanup");
	}
	#[tokio::test]
	async fn async_boolean() {
		let mut vec = Vec::<u8>::new();
		let thing = true;
		thing.serialize_async(&mut vec).await.unwrap();
		let r = &mut &vec[..];
		let other = bool::deserialize_async(r).await.unwrap();
		assert_eq!(thing, other, "values don't match");
		assert_eq!(*r, &[], "cleanup");
	}
	#[test]
	fn streaming_optional() {
		let mut vec = Vec::<u8>::new();
		let thing = Option::<Cow<'_, str>>::None;
		thing.serialize(&mut vec).unwrap();
		let r = &mut &vec[..];
		let other = Option::<Cow<'_, str>>::deserialize_owned(r).unwrap();
		assert_eq!(thing, other, "values don't match");
		assert_eq!(*r, &[], "cleanup");
	}
	#[test]
	fn in_place_optional() {
		let mut vec = Vec::<u8>::new();
		let thing = Option::Some(Cow::from("Hello, world!"));
		thing.serialize(&mut vec).unwrap();
		let r = &mut &vec[..];
		let other = Option::<Cow<'_, str>>::deserialize(r).unwrap();
		assert_eq!(thing, other, "values don't match");
		assert_eq!(*r, &[], "cleanup");
	}
	#[tokio::test]
	async fn async_optional() {
		let mut vec = Vec::<u8>::new();
		let thing = Option::Some(Cow::from("Hello, world!"));
		thing.serialize_async(&mut vec).await.unwrap();
		let r = &mut &vec[..];
		let other = Option::<Cow<'_, str>>::deserialize_async(r).await.unwrap();
		assert_eq!(thing, other, "values don't match");
		assert_eq!(*r, &[], "cleanup");
	}
}