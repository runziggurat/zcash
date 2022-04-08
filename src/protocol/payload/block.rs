//! Block-related types.

use crate::protocol::payload::{
    codec::Codec,
    inv::{InvHash, ObjectKind},
    read_n_bytes, Hash, ProtocolVersion, Tx, VarInt,
};

use std::{convert::TryInto, io};

use bytes::{Buf, BufMut};
use sha2::Digest;

/// The locator hash object, used to communicate chain state.
#[derive(Debug, PartialEq, Clone)]
pub struct LocatorHashes {
    /// The protocol version.
    pub version: ProtocolVersion,
    /// The block locator hashes describing current chain state.
    ///
    /// The order is from newest to genesis (dense to start, then sparse).
    pub block_locator_hashes: Vec<Hash>,
    /// The hash of the last desired block or header. Setting this to `0` will ask for as many
    /// blocks as possible.
    pub hash_stop: Hash,
}

impl LocatorHashes {
    /// Returns a new `LocatorHashes` instance with the current protocol version.
    pub fn new(block_locator_hashes: Vec<Hash>, hash_stop: Hash) -> Self {
        Self {
            version: ProtocolVersion::current(),
            block_locator_hashes,
            hash_stop,
        }
    }

    /// Returns an empty `LocatorHashes` instance.
    pub fn empty() -> Self {
        Self::new(Vec::new(), Hash::zeroed())
    }
}

impl Codec for LocatorHashes {
    fn encode<B: BufMut>(&self, buffer: &mut B) -> io::Result<()> {
        self.version.encode(buffer)?;
        self.block_locator_hashes.encode(buffer)?;
        self.hash_stop.encode(buffer)?;

        Ok(())
    }

    fn decode<B: Buf>(bytes: &mut B) -> io::Result<Self> {
        let version = ProtocolVersion::decode(bytes)?;
        let block_locator_hashes = Vec::decode(bytes)?;
        let hash_stop = Hash::decode(bytes)?;

        Ok(Self {
            version,
            block_locator_hashes,
            hash_stop,
        })
    }
}

/// A block, composed of its header and transactions.
#[derive(Debug, PartialEq, Clone)]
pub struct Block {
    /// The block's header.
    pub header: Header,
    /// The block's transactions.
    pub txs: Vec<Tx>,
}

impl Block {
    /// Calculates the double Sha256 hash for this block.
    pub fn double_sha256(&self) -> std::io::Result<Hash> {
        self.header.double_sha256()
    }

    /// Creates the testnet genesis block.
    pub fn testnet_genesis() -> Self {
        let mut cursor = std::io::Cursor::new(&crate::vectors::BLOCK_TESTNET_GENESIS_BYTES[..]);
        Block::decode(&mut cursor).unwrap()
    }

    /// Creates the testnet block at height 1.
    pub fn testnet_1() -> Self {
        let mut cursor = std::io::Cursor::new(&crate::vectors::BLOCK_TESTNET_0_000_001_BYTES[..]);
        Block::decode(&mut cursor).unwrap()
    }

    /// Creates the testnet block at height 2.
    pub fn testnet_2() -> Self {
        let mut cursor = std::io::Cursor::new(&crate::vectors::BLOCK_TESTNET_0_000_002_BYTES[..]);
        Block::decode(&mut cursor).unwrap()
    }

    /// Creates the testnet block at height 3.
    pub fn testnet_3() -> Self {
        let mut cursor = std::io::Cursor::new(&crate::vectors::BLOCK_TESTNET_0_000_003_BYTES[..]);
        Block::decode(&mut cursor).unwrap()
    }

    /// Creates the testnet block at height 4.
    pub fn testnet_4() -> Self {
        let mut cursor = std::io::Cursor::new(&crate::vectors::BLOCK_TESTNET_0_000_004_BYTES[..]);
        Block::decode(&mut cursor).unwrap()
    }

    /// Creates the testnet block at height 5.
    pub fn testnet_5() -> Self {
        let mut cursor = std::io::Cursor::new(&crate::vectors::BLOCK_TESTNET_0_000_005_BYTES[..]);
        Block::decode(&mut cursor).unwrap()
    }

    /// Creates the testnet block at height 6.
    pub fn testnet_6() -> Self {
        let mut cursor = std::io::Cursor::new(&crate::vectors::BLOCK_TESTNET_0_000_006_BYTES[..]);
        Block::decode(&mut cursor).unwrap()
    }

    /// Creates the testnet block at height 7.
    pub fn testnet_7() -> Self {
        let mut cursor = std::io::Cursor::new(&crate::vectors::BLOCK_TESTNET_0_000_007_BYTES[..]);
        Block::decode(&mut cursor).unwrap()
    }

    /// Creates the testnet block at height 8.
    pub fn testnet_8() -> Self {
        let mut cursor = std::io::Cursor::new(&crate::vectors::BLOCK_TESTNET_0_000_008_BYTES[..]);
        Block::decode(&mut cursor).unwrap()
    }

    /// Creates the testnet block at height 9.
    pub fn testnet_9() -> Self {
        let mut cursor = std::io::Cursor::new(&crate::vectors::BLOCK_TESTNET_0_000_009_BYTES[..]);
        Block::decode(&mut cursor).unwrap()
    }

    /// Creates the testnet block at height 10.
    pub fn testnet_10() -> Self {
        let mut cursor = std::io::Cursor::new(&crate::vectors::BLOCK_TESTNET_0_000_010_BYTES[..]);
        Block::decode(&mut cursor).unwrap()
    }

    /// Returns the first 11 testnet blocks.
    pub fn initial_testnet_blocks() -> Vec<Self> {
        vec![
            Self::testnet_genesis(),
            Self::testnet_1(),
            Self::testnet_2(),
            Self::testnet_3(),
            Self::testnet_4(),
            Self::testnet_5(),
            Self::testnet_6(),
            Self::testnet_7(),
            Self::testnet_8(),
            Self::testnet_9(),
            Self::testnet_10(),
        ]
    }

    /// Convenience function which creates the [`InvHash`] for this block.
    pub fn inv_hash(&self) -> InvHash {
        InvHash::new(ObjectKind::Block, self.double_sha256().unwrap())
    }
}

impl Codec for Block {
    fn encode<B: BufMut>(&self, buffer: &mut B) -> io::Result<()> {
        self.header.encode_without_tx_count(buffer)?;
        self.txs.encode(buffer)
    }

    fn decode<B: Buf>(bytes: &mut B) -> io::Result<Self> {
        let header = Header::decode_without_tx_count(bytes)?;
        let txs = Vec::decode(bytes)?;
        Ok(Self { header, txs })
    }
}

/// A list of block headers.
#[derive(Debug, PartialEq, Clone)]
pub struct Headers {
    pub headers: Vec<Header>,
}

impl Headers {
    /// Returns a new `Headers` instance.
    pub fn new(headers: Vec<Header>) -> Self {
        Self { headers }
    }

    /// Returns an empty `Headers` instance.
    pub fn empty() -> Self {
        Self {
            headers: Vec::new(),
        }
    }
}

impl Codec for Headers {
    fn encode<B: BufMut>(&self, buffer: &mut B) -> io::Result<()> {
        self.headers.encode(buffer)
    }

    fn decode<B: Buf>(bytes: &mut B) -> io::Result<Self> {
        let headers = Vec::decode(bytes)?;
        Ok(Self::new(headers))
    }
}

/// A block header, see the [Zcash protocol
/// spec](https://zips.z.cash/protocol/protocol.pdf#blockheader) for details.
#[derive(Debug, PartialEq, Clone)]
pub struct Header {
    /// The block version number.
    pub version: ProtocolVersion,
    /// The hash of the previous block.
    pub prev_block: Hash,
    /// The hash of the merkle root.
    pub merkle_root: Hash,
    /// Field usage varies depending on version, see spec.
    pub light_client_root: Hash,
    /// The block timestamp.
    pub timestamp: u32,
    /// An encoded version of the target threshold.
    pub bits: u32,
    /// The nonce used in the version messages, `Nonce(u64)`, is NOT the same as the nonce the
    /// block was generated with as it uses a `u32`.
    pub nonce: [u8; 32],
    /// The size of the Equihash solution in bytes (always `1344`).
    pub solution_size: VarInt,
    /// The Equihash solution.
    pub solution: [u8; 1344],
}

impl Codec for Header {
    fn encode<B: BufMut>(&self, buffer: &mut B) -> io::Result<()> {
        self.encode_without_tx_count(buffer)?;
        // Encode tx_count=0
        VarInt(0).encode(buffer)
    }

    fn decode<B: Buf>(bytes: &mut B) -> io::Result<Self>
    where
        Self: Sized,
    {
        let result = Self::decode_without_tx_count(bytes);

        // tx_count must be zero
        let tx_count = *VarInt::decode(bytes)?;
        if tx_count != 0 {
            return Err(std::io::Error::new(
                std::io::ErrorKind::InvalidData,
                format!("Message::Header.tx_count = {}, expected 0", tx_count),
            ));
        }

        result
    }
}

impl Header {
    /// Calculates the double Sha256 hash for this header.
    pub fn double_sha256(&self) -> std::io::Result<Hash> {
        let mut buffer = Vec::new();

        self.encode_without_tx_count(&mut buffer)?;

        let hash_bytes_1 = sha2::Sha256::digest(&buffer);
        let hash_bytes_2 = sha2::Sha256::digest(&hash_bytes_1);

        let hash = Hash::new(hash_bytes_2.try_into().unwrap());

        Ok(hash)
    }

    /// Encodes [Header] without the VarInt `tx_count=0`. This is useful for [Block] encoding which requires
    /// `tx_count=N`, as well as Hash calculation as it excludes `tx_count`.
    fn encode_without_tx_count<B: BufMut>(&self, buffer: &mut B) -> io::Result<()> {
        self.version.encode(buffer)?;
        self.prev_block.encode(buffer)?;
        self.merkle_root.encode(buffer)?;
        self.light_client_root.encode(buffer)?;

        buffer.put_u32_le(self.timestamp);
        buffer.put_u32_le(self.bits);
        buffer.put_slice(&self.nonce);

        self.solution_size.encode(buffer)?;
        buffer.put_slice(&self.solution);

        Ok(())
    }

    /// Decodes [Header] without consuming the VarInt `tx_count`. This is useful for [Block] decoding which
    /// requires the value to determine the number of transactions which follow in the body. [Header] on the
    /// otherhand requires that this value be 0. This gets asserted in Header::encode, making it unsuiteable
    /// for use by [Block].
    fn decode_without_tx_count<B: Buf>(bytes: &mut B) -> io::Result<Self> {
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
}

#[cfg(test)]
mod tests {
    use hex::FromHex;

    use super::*;
    use crate::vectors::*;

    use std::io::Cursor;

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
        let block_bytes = &BLOCK_TESTNET_0_000_001_BYTES[..];
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
    fn testnet_2_round_trip() {
        // Pre-overwinter.
        let block_bytes = &BLOCK_TESTNET_0_000_002_BYTES[..];
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
    fn testnet_3_round_trip() {
        // Pre-overwinter.
        let block_bytes = &BLOCK_TESTNET_0_000_003_BYTES[..];
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
    fn testnet_4_round_trip() {
        // Pre-overwinter.
        let block_bytes = &BLOCK_TESTNET_0_000_004_BYTES[..];
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
    fn testnet_5_round_trip() {
        // Pre-overwinter.
        let block_bytes = &BLOCK_TESTNET_0_000_005_BYTES[..];
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
    fn testnet_6_round_trip() {
        // Pre-overwinter.
        let block_bytes = &BLOCK_TESTNET_0_000_006_BYTES[..];
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
    fn testnet_7_round_trip() {
        // Pre-overwinter.
        let block_bytes = &BLOCK_TESTNET_0_000_007_BYTES[..];
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
    fn testnet_8_round_trip() {
        // Pre-overwinter.
        let block_bytes = &BLOCK_TESTNET_0_000_008_BYTES[..];
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
    fn testnet_9_round_trip() {
        // Pre-overwinter.
        let block_bytes = &BLOCK_TESTNET_0_000_009_BYTES[..];
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
    fn testnet_10_round_trip() {
        // Pre-overwinter.
        let block_bytes = &BLOCK_TESTNET_0_000_010_BYTES[..];
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
        let block_bytes = &BLOCK_TESTNET_0_207_500_BYTES[..];
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
        let block_bytes = &BLOCK_TESTNET_0_280_000_BYTES[..];
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
        let block_bytes = &BLOCK_TESTNET_0_584_000_BYTES[..];
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
        let block_bytes = &BLOCK_TESTNET_0_903_800_BYTES[..];
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
        let block_bytes = &BLOCK_TESTNET_1_028_500_BYTES[..];
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
    fn testnet_1599199_round_trip() {
        // Canopy.
        let block_bytes = &BLOCK_TESTNET_1_599_199_BYTES[..];
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
    fn testnet_1599200_round_trip() {
        // Canopy.
        let block_bytes = &BLOCK_TESTNET_1_599_200_BYTES[..];
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
    fn testnet_1599201_round_trip() {
        // Canopy.
        let block_bytes = &BLOCK_TESTNET_1_599_201_BYTES[..];
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
