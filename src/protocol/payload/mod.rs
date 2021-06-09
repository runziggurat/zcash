use chrono::{DateTime, NaiveDateTime, Utc};
use rand::{thread_rng, Rng};

use std::io::{self, Cursor, Read, Write};

pub mod addr;
pub use addr::Addr;

pub mod block;

pub mod inv;
pub use inv::Inv;

pub mod tx;
pub use tx::Tx;

pub mod version;
pub use version::Version;

pub mod reject;
pub use reject::Reject;

use crate::protocol::message::constants::MAX_MESSAGE_LEN;

use self::codec::Codec;

pub mod codec;

pub mod filter;
pub use filter::{FilterAdd, FilterLoad};

#[derive(Debug, PartialEq, Copy, Clone)]
pub struct Nonce(u64);

impl Default for Nonce {
    fn default() -> Self {
        Self(thread_rng().gen())
    }
}

impl Codec for Nonce {
    fn encode(&self, buffer: &mut Vec<u8>) -> io::Result<()> {
        buffer.write_all(&self.0.to_le_bytes())?;

        Ok(())
    }

    fn decode(bytes: &mut Cursor<&[u8]>) -> io::Result<Self> {
        let nonce = u64::from_le_bytes(read_n_bytes(bytes)?);

        Ok(Self(nonce))
    }
}

#[derive(Debug, PartialEq, Clone, Copy)]
struct ProtocolVersion(u32);

impl ProtocolVersion {
    fn current() -> Self {
        Self(170_013)
    }
}

impl Codec for ProtocolVersion {
    fn encode(&self, buffer: &mut Vec<u8>) -> io::Result<()> {
        buffer.write_all(&self.0.to_le_bytes())?;

        Ok(())
    }

    fn decode(bytes: &mut Cursor<&[u8]>) -> io::Result<Self> {
        let version = u32::from_le_bytes(read_n_bytes(bytes)?);

        Ok(Self(version))
    }
}

#[derive(Debug, PartialEq, Clone, Copy)]
struct VarInt(usize);

impl std::ops::Deref for VarInt {
    type Target = usize;

    fn deref(&self) -> &Self::Target {
        &self.0
    }
}

impl Codec for VarInt {
    fn encode(&self, buffer: &mut Vec<u8>) -> io::Result<()> {
        // length of the payload to be written.
        let l = self.0;
        match l {
            0x0000_0000..=0x0000_00fc => {
                buffer.write_all(&[l as u8])?;
            }
            0x0000_00fd..=0x0000_ffff => {
                buffer.write_all(&[0xfdu8])?;
                buffer.write_all(&(l as u16).to_le_bytes())?;
            }
            0x0001_0000..=0xffff_ffff => {
                buffer.write_all(&[0xfeu8])?;
                buffer.write_all(&(l as u32).to_le_bytes())?;
            }
            _ => {
                buffer.write_all(&[0xffu8])?;
                buffer.write_all(&(l as u64).to_le_bytes())?;
            }
        };

        Ok(())
    }

    fn decode(bytes: &mut Cursor<&[u8]>) -> io::Result<Self> {
        let flag = u8::from_le_bytes(read_n_bytes(bytes)?);

        let len = match flag {
            len @ 0x00..=0xfc => len as u64,
            0xfd => u16::from_le_bytes(read_n_bytes(bytes)?) as u64,
            0xfe => u32::from_le_bytes(read_n_bytes(bytes)?) as u64,
            0xff => u64::from_le_bytes(read_n_bytes(bytes)?) as u64,
        };

        if len > MAX_MESSAGE_LEN as u64 {
            return Err(io::Error::new(
                io::ErrorKind::InvalidData,
                format!(
                    "VarInt length of {} exceeds max message length of {}",
                    len, MAX_MESSAGE_LEN
                ),
            ));
        }

        Ok(VarInt(len as usize))
    }
}

#[derive(Debug, PartialEq, Clone)]
struct VarStr(String);

impl VarStr {
    fn encode(&self, buffer: &mut Vec<u8>) -> io::Result<()> {
        VarInt(self.0.len()).encode(buffer)?;
        buffer.write_all(self.0.as_bytes())?;

        Ok(())
    }

    fn decode(bytes: &mut Cursor<&[u8]>) -> io::Result<Self> {
        let str_len = VarInt::decode(bytes)?;

        if *str_len > MAX_MESSAGE_LEN {
            return Err(io::Error::new(
                io::ErrorKind::InvalidData,
                format!(
                    "VarStr length of {} exceeds max message length of {}",
                    *str_len, MAX_MESSAGE_LEN
                ),
            ));
        }

        let mut buffer = vec![0u8; str_len.0];
        bytes.read_exact(&mut buffer)?;

        Ok(VarStr(String::from_utf8(buffer).map_err(|err| {
            std::io::Error::new(std::io::ErrorKind::InvalidData, err.to_string())
        })?))
    }
}

#[derive(Debug, PartialEq, Copy, Clone)]
pub struct Hash([u8; 32]);

impl Hash {
    pub fn new(hash: [u8; 32]) -> Self {
        Hash(hash)
    }

    pub fn zeroed() -> Self {
        Self([0; 32])
    }
}

impl Codec for Hash {
    fn encode(&self, buffer: &mut Vec<u8>) -> io::Result<()> {
        buffer.write_all(&self.0)?;

        Ok(())
    }

    fn decode(bytes: &mut Cursor<&[u8]>) -> io::Result<Self> {
        let mut hash = Hash([0u8; 32]);
        bytes.read_exact(&mut hash.0)?;

        Ok(hash)
    }
}

pub fn read_n_bytes<const N: usize>(bytes: &mut Cursor<&[u8]>) -> io::Result<[u8; N]> {
    let mut buffer = [0u8; N];
    bytes.read_exact(&mut buffer)?;

    Ok(buffer)
}

pub fn read_timestamp(bytes: &mut Cursor<&[u8]>) -> io::Result<DateTime<Utc>> {
    let timestamp_i64 = i64::from_le_bytes(read_n_bytes(bytes)?);
    let timestamp = NaiveDateTime::from_timestamp_opt(timestamp_i64, 0)
        .ok_or_else(|| io::Error::new(io::ErrorKind::InvalidData, "Bad UTC timestamp"))?;
    Ok(DateTime::<Utc>::from_utc(timestamp, Utc))
}
