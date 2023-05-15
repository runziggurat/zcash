//! Inventory vector types.

use std::io;

use bytes::{Buf, BufMut};

use crate::protocol::payload::{codec::Codec, read_n_bytes, Hash};

/// An inventory vector.
#[derive(Debug, PartialEq, Eq, Clone)]
pub struct Inv {
    pub inventory: Vec<InvHash>,
}

impl Inv {
    /// Returns a new inventory vector from the supplied hashes.
    pub fn new(inventory: Vec<InvHash>) -> Self {
        Self { inventory }
    }

    // Returns a new empty inventory vector.
    pub fn empty() -> Self {
        Self::new(Vec::new())
    }
}

impl Codec for Inv {
    fn encode<B: BufMut>(&self, buffer: &mut B) -> io::Result<()> {
        self.inventory.encode(buffer)
    }

    fn decode<B: Buf>(bytes: &mut B) -> io::Result<Self> {
        Ok(Self {
            inventory: Vec::decode(bytes)?,
        })
    }
}

/// An inventory hash which refers to some advertised or requested data.
///
/// Bitcoin calls this an "inventory vector" but it is just a typed hash, not a
/// container, so we do not use that term to avoid confusion with `Vec<T>`.
#[derive(Debug, PartialEq, Eq, Copy, Clone)]
pub enum InvHash {
    /// Any data of this kind may be ignored.
    Error,
    /// The hash is that of a transaction.
    Tx(Hash),
    /// The hash is that of a block.
    Block(Hash),
    /// The hash is that of a block header.
    FilteredBlock(Hash),
}

impl InvHash {
    /// Returns the serialized Zcash network protocol code for the current variant.
    fn code(&self) -> u32 {
        match self {
            Self::Error => 0,
            Self::Tx(_) => 1,
            Self::Block(_) => 2,
            Self::FilteredBlock(_) => 3,
        }
    }
}

impl Codec for InvHash {
    fn encode<B: BufMut>(&self, buffer: &mut B) -> io::Result<()> {
        buffer.put_u32_le(self.code());

        match self {
            Self::Tx(hash) | Self::Block(hash) | Self::FilteredBlock(hash) => {
                hash.encode(buffer)?;
            }
            _ => (),
        }

        Ok(())
    }

    fn decode<B: Buf>(bytes: &mut B) -> io::Result<Self> {
        let value = u32::from_le_bytes(read_n_bytes(bytes)?);

        let kind = match value {
            0 => Self::Error,
            1 => Self::Tx(Hash::decode(bytes)?),
            2 => Self::Block(Hash::decode(bytes)?),
            3 => Self::FilteredBlock(Hash::decode(bytes)?),
            _ => {
                return Err(io::Error::new(
                    io::ErrorKind::InvalidData,
                    format!("unknown inv hash value type: {value}"),
                ))
            }
        };

        Ok(kind)
    }
}
