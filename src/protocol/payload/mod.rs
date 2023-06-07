//! Network message payload types.

use std::io;

use bytes::{Buf, BufMut};
use rand::{thread_rng, Rng};
use serde::{Deserialize, Serialize};
use time::OffsetDateTime;

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

use self::codec::Codec;
use crate::protocol::message::constants::{MAX_MESSAGE_LEN, PROTOCOL_VERSION};

pub mod codec;

pub mod filter;
pub use filter::{FilterAdd, FilterLoad};

/// A `u64`-backed nonce.
#[derive(Debug, PartialEq, Eq, Copy, Clone)]
pub struct Nonce(u64);

impl Default for Nonce {
    fn default() -> Self {
        Self(thread_rng().gen())
    }
}

impl Codec for Nonce {
    fn encode<B: BufMut>(&self, buffer: &mut B) -> io::Result<()> {
        buffer.put_u64_le(self.0);

        Ok(())
    }

    fn decode<B: Buf>(bytes: &mut B) -> io::Result<Self> {
        if bytes.remaining() < 8 {
            return Err(io::ErrorKind::InvalidData.into());
        }
        let nonce = bytes.get_u64_le();

        Ok(Self(nonce))
    }
}

/// Specifies the protocol version.
#[derive(Debug, PartialEq, Eq, Clone, Copy)]
pub struct ProtocolVersion(pub u32);

impl ProtocolVersion {
    /// The current protocol version.
    pub fn current() -> Self {
        Self(PROTOCOL_VERSION)
    }
}

impl Codec for ProtocolVersion {
    fn encode<B: BufMut>(&self, buffer: &mut B) -> io::Result<()> {
        buffer.put_u32_le(self.0);

        Ok(())
    }

    fn decode<B: Buf>(bytes: &mut B) -> io::Result<Self> {
        let version = u32::from_le_bytes(read_n_bytes(bytes)?);

        Ok(Self(version))
    }
}

/// A variable length integer.
#[derive(Debug, PartialEq, Eq, Clone, Copy)]
pub struct VarInt(usize);

impl VarInt {
    pub fn new(value: usize) -> Self {
        Self(value)
    }
}

impl std::ops::Deref for VarInt {
    type Target = usize;

    fn deref(&self) -> &Self::Target {
        &self.0
    }
}

impl Codec for VarInt {
    fn encode<B: BufMut>(&self, buffer: &mut B) -> io::Result<()> {
        // length of the payload to be written.
        let l = self.0;
        match l {
            0x0000_0000..=0x0000_00fc => {
                buffer.put_u8(l as u8);
            }
            0x0000_00fd..=0x0000_ffff => {
                buffer.put_u8(0xfdu8);
                buffer.put_u16_le(l as u16);
            }
            0x0001_0000..=0xffff_ffff => {
                buffer.put_u8(0xfeu8);
                buffer.put_u32_le(l as u32);
            }
            _ => {
                buffer.put_u8(0xffu8);
                buffer.put_u64_le(l as u64);
            }
        };

        Ok(())
    }

    fn decode<B: Buf>(bytes: &mut B) -> io::Result<Self> {
        let flag = u8::from_le_bytes(read_n_bytes(bytes)?);

        let len = match flag {
            len @ 0x00..=0xfc => len as u64,
            0xfd => u16::from_le_bytes(read_n_bytes(bytes)?) as u64,
            0xfe => u32::from_le_bytes(read_n_bytes(bytes)?) as u64,
            0xff => u64::from_le_bytes(read_n_bytes(bytes)?),
        };

        if len > MAX_MESSAGE_LEN as u64 {
            return Err(io::Error::new(
                io::ErrorKind::InvalidData,
                format!("VarInt length of {len} exceeds max message length of {MAX_MESSAGE_LEN}"),
            ));
        }

        Ok(VarInt(len as usize))
    }
}

/// A variable length string.
#[derive(Debug, PartialEq, Eq, Clone)]
pub struct VarStr(pub String);

impl VarStr {
    fn encode<B: BufMut>(&self, buffer: &mut B) -> io::Result<()> {
        VarInt(self.0.len()).encode(buffer)?;
        buffer.put(self.0.as_bytes());

        Ok(())
    }

    fn decode<B: Buf>(bytes: &mut B) -> io::Result<Self> {
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

        if bytes.remaining() < str_len.0 {
            return Err(io::ErrorKind::InvalidData.into());
        }

        let mut buffer = vec![0u8; str_len.0];
        bytes.copy_to_slice(&mut buffer);

        Ok(VarStr(String::from_utf8(buffer).map_err(|err| {
            std::io::Error::new(std::io::ErrorKind::InvalidData, err.to_string())
        })?))
    }
}

/// A general purpose hash of length `32`.
#[derive(Debug, PartialEq, Eq, Copy, Clone, Serialize, Deserialize)]
pub struct Hash([u8; 32]);

impl Hash {
    /// Creates a `Hash` instance.
    pub fn new(hash: [u8; 32]) -> Self {
        Hash(hash)
    }

    /// Returns a `Hash` with only `0`s.
    pub fn zeroed() -> Self {
        Self([0; 32])
    }
}

impl Codec for Hash {
    fn encode<B: BufMut>(&self, buffer: &mut B) -> io::Result<()> {
        buffer.put_slice(&self.0);

        Ok(())
    }

    fn decode<B: Buf>(bytes: &mut B) -> io::Result<Self> {
        if bytes.remaining() < 32 {
            return Err(io::ErrorKind::InvalidData.into());
        }

        let mut hash = Hash([0u8; 32]);
        bytes.copy_to_slice(&mut hash.0);

        Ok(hash)
    }
}

/// Reads `n` bytes from the bytes.
pub fn read_n_bytes<const N: usize, B: Buf>(bytes: &mut B) -> io::Result<[u8; N]> {
    if bytes.remaining() < N {
        return Err(io::ErrorKind::InvalidData.into());
    }

    let mut buffer = [0u8; N];
    bytes.copy_to_slice(&mut buffer);

    Ok(buffer)
}

/// Reads a timestamp encoded as 8 bytes.
pub fn read_timestamp<B: Buf>(bytes: &mut B) -> io::Result<OffsetDateTime> {
    let timestamp_i64 = i64::from_le_bytes(read_n_bytes(bytes)?);
    OffsetDateTime::from_unix_timestamp(timestamp_i64)
        .map_err(|_| io::Error::new(io::ErrorKind::InvalidData, "Bad UTC timestamp"))
}

/// Reads a timestamp encoded as 4 bytes.
pub fn read_short_timestamp<B: Buf>(bytes: &mut B) -> io::Result<OffsetDateTime> {
    let timestamp_u32 = u32::from_le_bytes(read_n_bytes(bytes)?);
    OffsetDateTime::from_unix_timestamp(timestamp_u32.into())
        .map_err(|_| io::Error::new(io::ErrorKind::InvalidData, "Bad UTC timestamp"))
}
