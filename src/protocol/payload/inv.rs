//! Inventory vector types.

use bytes::{Buf, BufMut};

use crate::protocol::payload::{codec::Codec, read_n_bytes, Hash};

use std::io;

/// An inventory vector.
#[derive(Debug, PartialEq, Clone)]
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

/// An inventory hash.
#[derive(Debug, PartialEq, Copy, Clone)]
pub struct InvHash {
    /// The object type linked to this inventory.
    kind: ObjectKind,
    /// The hash of the object.
    hash: Hash,
}

impl InvHash {
    /// Returns a new `InvHash` instance.
    pub fn new(kind: ObjectKind, hash: Hash) -> Self {
        Self { kind, hash }
    }
}

impl Codec for InvHash {
    fn encode<B: BufMut>(&self, buffer: &mut B) -> io::Result<()> {
        self.kind.encode(buffer)?;
        self.hash.encode(buffer)?;

        Ok(())
    }

    fn decode<B: Buf>(bytes: &mut B) -> io::Result<Self> {
        let kind = ObjectKind::decode(bytes)?;
        let hash = Hash::decode(bytes)?;

        Ok(Self { kind, hash })
    }
}

/// The inventory object kind.
#[derive(Debug, PartialEq, Clone, Copy)]
pub enum ObjectKind {
    /// Any data of this kind may be ignored.
    Error,
    /// The hash is that of a transaction.
    Tx,
    /// The hash is that of a block.
    Block,
    /// The hash is that of a block header.
    FilteredBlock,
}

impl Codec for ObjectKind {
    fn encode<B: BufMut>(&self, buffer: &mut B) -> io::Result<()> {
        let value: u32 = match self {
            Self::Error => 0,
            Self::Tx => 1,
            Self::Block => 2,
            Self::FilteredBlock => 3,
        };

        buffer.put_u32_le(value);

        Ok(())
    }

    fn decode<B: Buf>(bytes: &mut B) -> io::Result<Self> {
        let value = u32::from_le_bytes(read_n_bytes(bytes)?);

        let kind = match value {
            0 => Self::Error,
            1 => Self::Tx,
            2 => Self::Block,
            3 => Self::FilteredBlock,
            _ => {
                return Err(io::Error::new(
                    io::ErrorKind::InvalidData,
                    "ObjectKind is not known",
                ))
            }
        };

        Ok(kind)
    }
}
