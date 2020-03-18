extern crate hex;

use std::io::{Read, Write, Result as IOResult, Cursor};

use bitcoin_spv::types::Hash256Digest;

#[derive(PartialEq, Eq, PartialOrd, Ord, Clone, Debug)]
pub struct VarInt(pub u64, pub u8);   // number and byte-length

impl VarInt {
    pub fn new(number: u64) -> Self {
        let byte_len = VarInt::byte_len(number);
        VarInt(number, byte_len)
    }

    pub fn byte_len(number: u64) -> u8 {
        match number {
            0..=0xfc => 1,
            0xfd..=0xffff => 3,
            0x10000..=0xffff_ffff => 5,
            _ => 9
        }
    }

    pub fn prefix_from_len(number: u8) -> Option<u8> {
        match number {
             3 =>  Some(0xfd),
             5 =>  Some(0xfe),
             9 =>  Some(0xff),
             _ => None
        }
    }

    pub fn len_from_prefix(number: u8) -> u8 {
        match number {
            0..=0xfc => 1,
            0xfd => 3,
            0xfe => 5,
            0xff => 9
        }
    }
}

pub trait Ser {
    fn serialized_length(&self) -> IOResult<usize>;

    fn deserialize<T>(reader: &mut T, _limit: usize) -> IOResult<Self>
    where
        T: Read,
        Self: std::marker::Sized;

    fn serialize<T>(&self, writer: &mut T) -> IOResult<usize>
    where
        T: Write;

    fn serialize_hex(&self) -> IOResult<String> {
        let mut v: Vec<u8> = vec![];
        self.serialize(&mut v)?;
        Ok(hex::encode(v))
    }

    fn deserialize_hex(s: String) -> IOResult<Self>
    where
        Self: std::marker::Sized
    {
        let v: Vec<u8> = hex::decode(s).unwrap();
        let mut cursor = Cursor::new(v);
        Ok(Self::deserialize(&mut cursor, 0)?)
    }
}

impl Ser for VarInt {

    fn serialized_length(&self) -> IOResult<usize> {
        Ok(self.1 as usize)
    }

    fn deserialize<T>(reader: &mut T, _limit: usize) -> IOResult<VarInt>
    where
        T: Read
    {
        let mut prefix = [0u8; 1];
        reader.read_exact(&mut prefix)?;  // read at most one byte

        let len = VarInt::len_from_prefix(prefix[0]);
        if len == 1 {
            return Ok(VarInt(prefix[0] as u64, 1u8));
        }

        let mut buf = [0u8; 8];
        let mut body = reader.take(len as u64 - 1); // minus 1 to account for prefix
        let _ = body.read(&mut buf)?;
        Ok(VarInt(u64::from_le_bytes(buf), len))
    }

    fn serialize<T>(&self, writer: &mut T) -> IOResult<usize>
    where
        T: Write
    {
        match VarInt::prefix_from_len(self.1) {
            Some(prefix) => {
                let body = self.0.to_le_bytes();
                let mut len = writer.write(&[prefix])?;
                len += writer.write(&body[..self.1 as usize - 1])?; // adjust by one for prefix
                Ok(len)
            },
            None => writer.write(&[self.0 as u8])
        }
    }
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct Script {
    pub length: VarInt,
    pub body: Vec<u8>
}

impl Script {
    pub fn len(&self) -> usize {
        self.length.0 as usize
    }

    pub fn is_empty(&self) -> bool {
        self.len() == 0
    }

    pub fn null() -> Self {
        Script::new(vec![])
    }

    pub fn new(script: Vec<u8>) -> Self {
        Script{
            length: VarInt::new(script.len() as u64),
            body: script
        }
    }
}

impl<T> From<T> for Script
where
    T: Into<Vec<u8>>
{
    fn from(v: T) -> Self {
        Script::new(v.into())
    }
}

impl Ser for Script {
    fn serialized_length(&self) -> IOResult<usize> {
        let mut len = self.length.serialized_length()?;
        len += self.body.serialized_length()?;
        Ok(len)
    }

    fn deserialize<T>(reader: &mut T, _limit: usize) -> IOResult<Self>
    where
        T: Read,
        Self: std::marker::Sized
    {
        let length = VarInt::deserialize(reader, 0)?;
        let limit = length.0;
        Ok(Script{
            length,
            body: Vec::<u8>::deserialize(reader, limit as usize)?
        })
    }

    fn serialize<T>(&self, writer: &mut T) -> IOResult<usize>
    where
        T: Write
    {
        let mut len = self.length.serialize(writer)?;
        len += self.body.serialize(writer)?;
        Ok(len)
    }
}

impl Ser for Hash256Digest {
    fn serialized_length(&self) -> IOResult<usize> {
        Ok(32)
    }

    fn deserialize<T>(reader: &mut T, _limit: usize) -> IOResult<Self>
    where
        T: Read,
        Self: std::marker::Sized
    {
        let mut buf = Hash256Digest::default();
        reader.read_exact(&mut buf)?;
        Ok(buf)
    }

    fn serialize<T>(&self, writer: &mut T) -> IOResult<usize>
    where
        T: Write
    {
        writer.write(self)
    }
}

impl Ser for u8 {
    fn serialized_length(&self) -> IOResult<usize> {
        Ok(1)
    }

    fn deserialize<T>(reader: &mut T, _limit: usize) -> IOResult<Self>
    where
        T: Read,
        Self: std::marker::Sized
    {
        let mut buf = [0u8; 1];
        reader.read_exact(&mut buf)?;
        Ok(u8::from_le_bytes(buf))
    }

    fn serialize<T>(&self, writer: &mut T) -> IOResult<usize>
    where
        T: Write
    {
        writer.write(&self.to_le_bytes())
    }
}

impl Ser for u32 {
    fn serialized_length(&self) -> IOResult<usize> {
        Ok(4)
    }

    fn deserialize<T>(reader: &mut T, _limit: usize) -> IOResult<Self>
    where
        T: Read,
        Self: std::marker::Sized
    {
        let mut buf = [0u8; 4];
        reader.read_exact(&mut buf)?;
        Ok(u32::from_le_bytes(buf))
    }

    fn serialize<T>(&self, writer: &mut T) -> IOResult<usize>
    where
        T: Write
    {
        writer.write(&self.to_le_bytes())
    }
}

impl Ser for u64 {
    fn serialized_length(&self) -> IOResult<usize> {
        Ok(8)
    }

    fn deserialize<T>(reader: &mut T, _limit: usize) -> IOResult<Self>
    where
        T: Read,
        Self: std::marker::Sized
    {
        let mut buf = [0u8; 8];
        reader.read_exact(&mut buf)?;
        Ok(u64::from_le_bytes(buf))
    }

    fn serialize<T>(&self, writer: &mut T) -> IOResult<usize>
    where
        T: Write
    {
        writer.write(&self.to_le_bytes())
    }
}

impl<A> Ser for Vec<A>
where
    A: Ser
{
    fn serialized_length(&self) -> IOResult<usize> {
        // panics. TODO: fix later
        Ok(self.iter().map(|v| v.serialized_length().unwrap()).sum())
    }

    fn deserialize<T>(reader: &mut T, limit: usize) -> IOResult<Self>
    where
        T: Read,
        Self: std::marker::Sized
    {
        let mut v = vec![];
        for _ in 0..limit {
            v.push(A::deserialize(reader, 0)?);
        }
        Ok(v)
    }

    fn serialize<T>(&self, writer: &mut T) -> IOResult<usize>
    where
        T: Write
    {
        Ok(self.iter().map(|v| v.serialize(writer).unwrap()).sum())
    }
}

#[cfg(test)]
mod test {
    use super::*;

    #[test]
    fn it_serializes_and_derializes_var_ints() {
        let cases = [
            (VarInt(1, 1), "01"),
            (VarInt(1, 3), "fd0100"),
            (VarInt(1, 5), "fe01000000"),
            (VarInt(1, 9), "ff0100000000000000"),
            (VarInt(0xaabbccdd, 9), "ffddccbbaa00000000")
        ];
        for case in cases.iter() {
            assert_eq!(case.0.serialize_hex().unwrap(), case.1.to_owned());
            assert_eq!(VarInt::deserialize_hex(case.1.to_owned()).unwrap(), case.0);
        }
    }

}