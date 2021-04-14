use crate::protocol::payload::{read_n_bytes, Hash, ProtocolVersion, VarInt};

use chrono::{DateTime, NaiveDateTime, Utc};

use std::io::{self, Cursor, Write};

#[derive(Debug)]
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

#[derive(Debug)]
pub struct Headers {
    count: VarInt,
    headers: Vec<Header>,
}

impl Headers {
    pub fn empty() -> Self {
        Headers {
            count: VarInt(0),
            headers: Vec::new(),
        }
    }

    pub fn encode(&self, buffer: &mut Vec<u8>) -> io::Result<()> {
        self.count.encode(buffer)?;

        for header in &self.headers {
            header.encode(buffer)?
        }

        Ok(())
    }

    pub fn decode(bytes: &mut Cursor<&[u8]>) -> io::Result<Self> {
        let count = VarInt::decode(bytes)?;
        let mut headers = Vec::with_capacity(count.0);

        for _ in 0..count.0 {
            let header = Header::decode(bytes)?;
            headers.push(header);
        }

        Ok(Self { count, headers })
    }
}

#[derive(Debug)]
struct Header {
    version: ProtocolVersion,
    prev_block: Hash,
    merkle_root: Hash,
    light_client_root: Hash,
    timestamp: DateTime<Utc>,
    bits: u32,
    // The nonce used in the version messages (`Nonce(u64)`) is NOT the same as the nonce the block
    // was generated with as it uses a `u32`.
    nonce: u32,
    txn_count: VarInt,
}

impl Header {
    fn encode(&self, buffer: &mut Vec<u8>) -> io::Result<()> {
        self.version.encode(buffer)?;
        self.prev_block.encode(buffer)?;
        self.merkle_root.encode(buffer)?;
        self.light_client_root.encode(buffer)?;

        buffer.write_all(&self.timestamp.timestamp().to_le_bytes())?;
        buffer.write_all(&self.bits.to_le_bytes())?;
        buffer.write_all(&self.nonce.to_le_bytes())?;

        self.txn_count.encode(buffer)?;

        Ok(())
    }

    fn decode(bytes: &mut Cursor<&[u8]>) -> io::Result<Self> {
        let version = ProtocolVersion::decode(bytes)?;
        let prev_block = Hash::decode(bytes)?;
        let merkle_root = Hash::decode(bytes)?;
        let light_client_root = Hash::decode(bytes)?;

        let timestamp = i64::from_le_bytes(read_n_bytes(bytes)?);
        let dt = DateTime::<Utc>::from_utc(NaiveDateTime::from_timestamp(timestamp, 0), Utc);

        let bits = u32::from_le_bytes(read_n_bytes(bytes)?);
        let nonce = u32::from_le_bytes(read_n_bytes(bytes)?);

        let txn_count = VarInt::decode(bytes)?;

        Ok(Self {
            version,
            prev_block,
            merkle_root,
            light_client_root,
            timestamp: dt,
            bits,
            nonce,
            txn_count,
        })
    }
}
