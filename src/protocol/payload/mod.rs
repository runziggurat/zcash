use rand::{thread_rng, Rng};

use std::io::{self, Cursor, Read, Write};

pub mod addr;
pub use addr::Addr;

pub mod version;
pub use version::Version;

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

fn write_string(buffer: &mut Vec<u8>, s: &str) -> io::Result<usize> {
    // Bitcoin "CompactSize" encoding.
    let l = s.len();
    let cs_len = match l {
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

    buffer.write_all(s.as_bytes())?;

    Ok(l + cs_len)
}

fn decode_string(bytes: &mut Cursor<&[u8]>) -> io::Result<String> {
    let flag = u8::from_le_bytes(read_n_bytes(bytes)?);

    let len = match flag {
        len @ 0x00..=0xfc => len as u64,
        0xfd => u16::from_le_bytes(read_n_bytes(bytes)?) as u64,
        0xfe => u32::from_le_bytes(read_n_bytes(bytes)?) as u64,
        0xff => u64::from_le_bytes(read_n_bytes(bytes)?),
    };

    let mut buffer = vec![0u8; len as usize];
    bytes.read_exact(&mut buffer)?;
    Ok(String::from_utf8(buffer).expect("invalid utf-8"))
}

fn read_n_bytes<const N: usize>(bytes: &mut Cursor<&[u8]>) -> io::Result<[u8; N]> {
    let mut buffer = [0u8; N];
    bytes.read_exact(&mut buffer)?;

    Ok(buffer)
}
