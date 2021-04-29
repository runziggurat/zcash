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

#[derive(Debug, PartialEq, Copy, Clone)]
pub struct Nonce(u64);

impl Default for Nonce {
    fn default() -> Self {
        Self(thread_rng().gen())
    }
}

impl Nonce {
    pub fn encode(&self, buffer: &mut Vec<u8>) -> io::Result<()> {
        buffer.write_all(&self.0.to_le_bytes())?;

        Ok(())
    }

    pub fn decode(bytes: &mut Cursor<&[u8]>) -> io::Result<Self> {
        let nonce = u64::from_le_bytes(read_n_bytes(bytes)?);

        Ok(Self(nonce))
    }
}

#[derive(Debug, PartialEq)]
struct ProtocolVersion(u32);

impl ProtocolVersion {
    fn encode(&self, buffer: &mut Vec<u8>) -> io::Result<()> {
        buffer.write_all(&self.0.to_le_bytes())?;

        Ok(())
    }

    fn decode(bytes: &mut Cursor<&[u8]>) -> io::Result<Self> {
        let version = u32::from_le_bytes(read_n_bytes(bytes)?);

        Ok(Self(version))
    }
}

// TODO: impl Deref
#[derive(Debug, PartialEq)]
struct VarInt(usize);

impl VarInt {
    fn encode(&self, buffer: &mut Vec<u8>) -> io::Result<usize> {
        // length of the payload to be written.
        let l = self.0;
        let bytes_written = match l {
            0x0000_0000..=0x0000_00fc => {
                buffer.write_all(&[l as u8])?;
                1 // bytes written
            }
            0x0000_00fd..=0x0000_ffff => {
                buffer.write_all(&[0xfdu8])?;
                buffer.write_all(&(l as u16).to_le_bytes())?;
                3
            }
            0x0001_0000..=0xffff_ffff => {
                buffer.write_all(&[0xfeu8])?;
                buffer.write_all(&(l as u32).to_le_bytes())?;
                5
            }
            _ => {
                buffer.write_all(&[0xffu8])?;
                buffer.write_all(&(l as u64).to_le_bytes())?;
                9
            }
        };

        Ok(bytes_written)
    }

    fn decode(bytes: &mut Cursor<&[u8]>) -> io::Result<Self> {
        let flag = u8::from_le_bytes(read_n_bytes(bytes)?);

        let len = match flag {
            len @ 0x00..=0xfc => len as u64,
            0xfd => u16::from_le_bytes(read_n_bytes(bytes)?) as u64,
            0xfe => u32::from_le_bytes(read_n_bytes(bytes)?) as u64,
            0xff => u64::from_le_bytes(read_n_bytes(bytes)?) as u64,
        };

        Ok(VarInt(len as usize))
    }
}

#[derive(Debug)]
struct VarStr(String);

impl VarStr {
    fn encode(&self, buffer: &mut Vec<u8>) -> io::Result<usize> {
        let int_len = VarInt(self.0.len()).encode(buffer)?;
        buffer.write_all(self.0.as_bytes())?;

        Ok(int_len + self.0.len())
    }

    fn decode(bytes: &mut Cursor<&[u8]>) -> io::Result<Self> {
        let str_len = VarInt::decode(bytes)?;
        let mut buffer = vec![0u8; str_len.0];
        bytes.read_exact(&mut buffer)?;

        Ok(VarStr(String::from_utf8(buffer).expect("invalid utf-8")))
    }
}

#[derive(Debug, PartialEq)]
struct Hash([u8; 32]);

impl Hash {
    pub fn new(hash: [u8; 32]) -> Self {
        Hash(hash)
    }

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

fn read_n_bytes<const N: usize>(bytes: &mut Cursor<&[u8]>) -> io::Result<[u8; N]> {
    let mut buffer = [0u8; N];
    bytes.read_exact(&mut buffer)?;

    Ok(buffer)
}
