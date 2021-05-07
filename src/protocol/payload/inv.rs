use crate::protocol::payload::{codec::Codec, read_n_bytes, Hash};

use std::io::{self, Cursor, Write};

#[derive(Debug)]
pub struct Inv {
    inventory: Vec<InvHash>,
}

impl Codec for Inv {
    fn encode(&self, buffer: &mut Vec<u8>) -> io::Result<()> {
        self.inventory.encode(buffer)
    }

    fn decode(bytes: &mut Cursor<&[u8]>) -> io::Result<Self> {
        Ok(Self {
            inventory: Vec::decode(bytes)?,
        })
    }
}

pub struct InvHash {
    kind: ObjectKind,
    hash: Hash,
}

impl Codec for InvHash {
    fn encode(&self, buffer: &mut Vec<u8>) -> io::Result<()> {
        self.kind.encode(buffer)?;
        self.hash.encode(buffer)?;

        Ok(())
    }

    fn decode(bytes: &mut Cursor<&[u8]>) -> io::Result<Self> {
        let kind = ObjectKind::decode(bytes)?;
        let hash = Hash::decode(bytes)?;

        Ok(Self { kind, hash })
    }
}

#[derive(Debug)]
enum ObjectKind {
    Error,
    Tx,
    Block,
    FilteredBlock,
}

impl Codec for ObjectKind {
    fn encode(&self, buffer: &mut Vec<u8>) -> io::Result<()> {
        let value: u32 = match self {
            Self::Error => 0,
            Self::Tx => 1,
            Self::Block => 2,
            Self::FilteredBlock => 3,
        };

        buffer.write_all(&value.to_le_bytes())?;

        Ok(())
    }

    fn decode(bytes: &mut Cursor<&[u8]>) -> io::Result<Self> {
        let value = u32::from_le_bytes(read_n_bytes(bytes)?);

        let kind = match value {
            0 => Self::Error,
            1 => Self::Tx,
            2 => Self::Block,
            3 => Self::FilteredBlock,
            _ => unreachable!(),
        };

        Ok(kind)
    }
}
