use crate::protocol::payload::{Hash, ProtocolVersion, VarInt};

use std::io::{self, Cursor};

pub struct LocatorHashes {
    version: ProtocolVersion,
    count: VarInt,
    block_locator_hashes: Vec<Hash>,
    hash_stop: Hash,
}

impl LocatorHashes {
    pub fn encode(&self, buffer: &mut Vec<u8>) -> io::Result<()> {
        self.version.encode(buffer)?;
        self.count.encode(buffer)?;

        for hash in &self.block_locator_hashes {
            hash.encode(buffer)?;
        }

        self.hash_stop.encode(buffer)?;

        Ok(())
    }

    pub fn decode(bytes: &mut Cursor<&[u8]>) -> io::Result<Self> {
        let version = ProtocolVersion::decode(bytes)?;
        let count = VarInt::decode(bytes)?;
        let mut block_locator_hashes = Vec::with_capacity(count.0);

        for _ in 0..count.0 {
            let hash = Hash::decode(bytes)?;
            block_locator_hashes.push(hash);
        }

        let hash_stop = Hash::decode(bytes)?;

        Ok(Self {
            version,
            count,
            block_locator_hashes,
            hash_stop,
        })
    }
}
