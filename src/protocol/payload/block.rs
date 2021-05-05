use crate::protocol::payload::{read_n_bytes, Hash, ProtocolVersion, Tx, VarInt};

use std::{
    convert::TryInto,
    io::{self, Cursor, Write},
};

use sha2::Digest;

#[derive(Debug)]
pub struct LocatorHashes {
    version: ProtocolVersion,
    block_locator_hashes: Vec<Hash>,
    hash_stop: Hash,
}

impl LocatorHashes {
    pub fn new(block_locator_hashes: Vec<Hash>, hash_stop: Hash) -> Self {
        Self {
            version: ProtocolVersion::current(),
            block_locator_hashes,
            hash_stop,
        }
    }

    pub fn empty() -> Self {
        Self::new(Vec::new(), Hash::zeroed())
    }

    pub fn encode(&self, buffer: &mut Vec<u8>) -> io::Result<()> {
        self.version.encode(buffer)?;
        VarInt(self.block_locator_hashes.len()).encode(buffer)?;

        for hash in &self.block_locator_hashes {
            hash.encode(buffer)?;
        }

        self.hash_stop.encode(buffer)?;

        Ok(())
    }

    pub fn decode(bytes: &mut Cursor<&[u8]>) -> io::Result<Self> {
        let version = ProtocolVersion::decode(bytes)?;
        let count = *VarInt::decode(bytes)?;
        let mut block_locator_hashes = Vec::with_capacity(count);

        for _ in 0..count {
            let hash = Hash::decode(bytes)?;
            block_locator_hashes.push(hash);
        }

        let hash_stop = Hash::decode(bytes)?;

        Ok(Self {
            version,
            block_locator_hashes,
            hash_stop,
        })
    }
}

#[derive(Debug, PartialEq)]
pub struct Block {
    pub header: Header,
    txs: Vec<Tx>,
}

impl Block {
    pub fn encode(&self, buffer: &mut Vec<u8>) -> io::Result<()> {
        self.header.encode(buffer)?;
        VarInt(self.txs.len()).encode(buffer)?;

        for tx in &self.txs {
            tx.encode(buffer)?;
        }

        Ok(())
    }

    pub fn decode(bytes: &mut Cursor<&[u8]>) -> io::Result<Self> {
        let header = Header::decode(bytes)?;
        let tx_count = *VarInt::decode(bytes)?;
        let mut txs = Vec::with_capacity(tx_count);

        for _ in 0..tx_count {
            let tx = Tx::decode(bytes)?;
            txs.push(tx);
        }

        Ok(Self { header, txs })
    }

    /// Calculates the double Sha256 hash for this [Block]
    pub fn double_sha256(&self) -> std::io::Result<Hash> {
        self.header.double_sha256()
    }
}

#[derive(Debug)]
pub struct Headers {
    headers: Vec<Header>,
}

impl Headers {
    pub fn new(headers: Vec<Header>) -> Self {
        Self { headers }
    }

    pub fn empty() -> Self {
        Headers {
            headers: Vec::new(),
        }
    }

    pub fn encode(&self, buffer: &mut Vec<u8>) -> io::Result<()> {
        VarInt(self.headers.len()).encode(buffer)?;

        for header in &self.headers {
            header.encode(buffer)?;
            // This encodes the tx_count, which is always 0 for the Header message
            // (since we don't send the tx vector, unlike a Block message)
            VarInt(0).encode(buffer)?;
        }

        Ok(())
    }

    pub fn decode(bytes: &mut Cursor<&[u8]>) -> io::Result<Self> {
        let count = *VarInt::decode(bytes)?;
        let mut headers = Vec::with_capacity(count);

        for _ in 0..count {
            let header = Header::decode(bytes)?;
            // The tx_count must always be 0 for a Header message
            let tx_count = *VarInt::decode(bytes)?;
            if tx_count != 0 {
                return Err(std::io::Error::new(
                    std::io::ErrorKind::InvalidData,
                    format!("Message::Header.tx_count = {}, expected 0", tx_count),
                ));
            }
            headers.push(header);
        }

        Ok(Self::new(headers))
    }
}

#[derive(Debug, PartialEq)]
pub struct Header {
    version: ProtocolVersion,
    prev_block: Hash,
    merkle_root: Hash,
    light_client_root: Hash,
    timestamp: u32,
    bits: u32,
    // The nonce used in the version messages (`Nonce(u64)`) is NOT the same as the nonce the block
    // was generated with as it uses a `u32`.
    nonce: [u8; 32],
    solution_size: VarInt,
    solution: [u8; 1344],
}

impl Header {
    fn encode(&self, buffer: &mut Vec<u8>) -> io::Result<()> {
        self.version.encode(buffer)?;
        self.prev_block.encode(buffer)?;
        self.merkle_root.encode(buffer)?;
        self.light_client_root.encode(buffer)?;

        buffer.write_all(&self.timestamp.to_le_bytes())?;
        buffer.write_all(&self.bits.to_le_bytes())?;
        buffer.write_all(&self.nonce)?;

        self.solution_size.encode(buffer)?;
        buffer.write_all(&self.solution)?;

        Ok(())
    }

    fn decode(bytes: &mut Cursor<&[u8]>) -> io::Result<Self> {
        let version = ProtocolVersion::decode(bytes)?;
        let prev_block = Hash::decode(bytes)?;
        let merkle_root = Hash::decode(bytes)?;
        let light_client_root = Hash::decode(bytes)?;

        let timestamp = u32::from_le_bytes(read_n_bytes(bytes)?);

        let bits = u32::from_le_bytes(read_n_bytes(bytes)?);
        let nonce = read_n_bytes(bytes)?;

        let solution_size = VarInt::decode(bytes)?;
        let solution = read_n_bytes(bytes)?;

        Ok(Self {
            version,
            prev_block,
            merkle_root,
            light_client_root,
            timestamp,
            bits,
            nonce,
            solution_size,
            solution,
        })
    }

    /// Calculates the double Sha256 hash for [Header]
    fn double_sha256(&self) -> std::io::Result<Hash> {
        let mut buffer = Vec::new();

        self.encode(&mut buffer)?;

        let hash_bytes_1 = sha2::Sha256::digest(&buffer);
        let hash_bytes_2 = sha2::Sha256::digest(&hash_bytes_1);

        let hash = Hash::new(hash_bytes_2.try_into().unwrap());

        Ok(hash)
    }
}

#[cfg(test)]
mod tests {
    use hex::FromHex;

    use super::*;
    use crate::vectors::*;

    #[test]
    #[ignore]
    fn testnet_genesis_round_trip() {
        let block_bytes = &BLOCK_TESTNET_GENESIS_BYTES[..];
        let mut bytes = Cursor::new(block_bytes);

        let mut buffer = Vec::new();
        Block::decode(&mut bytes)
            .unwrap()
            .encode(&mut buffer)
            .unwrap();

        assert_eq!(block_bytes, buffer);
    }

    #[test]
    #[ignore]
    fn testnet_1_round_trip() {
        // Pre-overwinter.
        let block_bytes = &BLOCK_TESTNET_1_BYTES[..];
        let mut bytes = Cursor::new(block_bytes);

        let mut buffer = Vec::new();
        Block::decode(&mut bytes)
            .unwrap()
            .encode(&mut buffer)
            .unwrap();

        assert_eq!(block_bytes, buffer);
    }

    #[test]
    #[ignore]
    fn testnet_207500_round_trip() {
        // Overwinter.
        let block_bytes = &BLOCK_TESTNET_207500_BYTES[..];
        let mut bytes = Cursor::new(block_bytes);

        let mut buffer = Vec::new();
        Block::decode(&mut bytes)
            .unwrap()
            .encode(&mut buffer)
            .unwrap();

        assert_eq!(block_bytes.len(), buffer.len());
    }

    #[test]
    #[ignore]
    fn testnet_280000_round_trip() {
        // Sapling.
        let block_bytes = &BLOCK_TESTNET_280000_BYTES[..];
        let mut bytes = Cursor::new(block_bytes);

        let mut buffer = Vec::new();
        Block::decode(&mut bytes)
            .unwrap()
            .encode(&mut buffer)
            .unwrap();

        assert_eq!(block_bytes, buffer);
    }

    #[test]
    #[ignore]
    fn testnet_584000_round_trip() {
        // Blossom.
        let block_bytes = &BLOCK_TESTNET_584000_BYTES[..];
        let mut bytes = Cursor::new(block_bytes);

        let mut buffer = Vec::new();
        Block::decode(&mut bytes)
            .unwrap()
            .encode(&mut buffer)
            .unwrap();

        assert_eq!(block_bytes, buffer);
    }

    #[test]
    #[ignore]
    fn testnet_903800_round_trip() {
        // Heartwood.
        let block_bytes = &BLOCK_TESTNET_903800_BYTES[..];
        let mut bytes = Cursor::new(block_bytes);

        let mut buffer = Vec::new();
        Block::decode(&mut bytes)
            .unwrap()
            .encode(&mut buffer)
            .unwrap();

        assert_eq!(block_bytes, buffer);
    }

    #[test]
    #[ignore]
    fn testnet_1028500_round_trip() {
        // Canopy.
        let block_bytes = &BLOCK_TESTNET_1028500_BYTES[..];
        let mut bytes = Cursor::new(block_bytes);

        let mut buffer = Vec::new();
        Block::decode(&mut bytes)
            .unwrap()
            .encode(&mut buffer)
            .unwrap();

        assert_eq!(block_bytes, buffer);
    }

    #[test]
    #[ignore]
    fn testnet_genesis_block_hash() {
        let mut bytes = Cursor::new(&BLOCK_TESTNET_GENESIS_BYTES[..]);
        let hash = Block::decode(&mut bytes).unwrap().double_sha256().unwrap();

        let mut expected_bytes =
            Vec::<u8>::from_hex("05a60a92d99d85997cce3b87616c089f6124d7342af37106edc76126334a2c38")
                .unwrap();
        expected_bytes.reverse();

        let expected = Hash::new(expected_bytes.try_into().unwrap());

        assert_eq!(expected, hash);
    }
}
