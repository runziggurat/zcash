//! Test vectors ordered by block height.
//!
//! Please note, these vectors have been copied across from [zebra](https://github.com/ZcashFoundation/zebra/tree/main/zebra-test/src/vectors).

use hex::FromHex;
use lazy_static::lazy_static;

lazy_static! {
    /// Testnet genesis block (pre-overwinter).
    pub static ref BLOCK_TESTNET_GENESIS_BYTES: Vec<u8> =
        <Vec<u8>>::from_hex(include_str!("block-test-0-000-000.txt").trim()).unwrap();
    /// Testnet block at height `1` (pre-overwinter).
    pub static ref BLOCK_TESTNET_0_000_001_BYTES: Vec<u8> =
        <Vec<u8>>::from_hex(include_str!("block-test-0-000-001.txt").trim()).unwrap();
    /// Testnet block at height `2` (pre-overwinter).
    pub static ref BLOCK_TESTNET_0_000_002_BYTES: Vec<u8> =
        <Vec<u8>>::from_hex(include_str!("block-test-0-000-002.txt").trim()).unwrap();
    /// Testnet block at height `3` (pre-overwinter).
    pub static ref BLOCK_TESTNET_0_000_003_BYTES: Vec<u8> =
        <Vec<u8>>::from_hex(include_str!("block-test-0-000-003.txt").trim()).unwrap();
    /// Testnet block at height `4` (pre-overwinter).
    pub static ref BLOCK_TESTNET_0_000_004_BYTES: Vec<u8> =
        <Vec<u8>>::from_hex(include_str!("block-test-0-000-004.txt").trim()).unwrap();
    /// Testnet block at height `5` (pre-overwinter).
    pub static ref BLOCK_TESTNET_0_000_005_BYTES: Vec<u8> =
        <Vec<u8>>::from_hex(include_str!("block-test-0-000-005.txt").trim()).unwrap();
    /// Testnet block at height `6` (pre-overwinter).
    pub static ref BLOCK_TESTNET_0_000_006_BYTES: Vec<u8> =
        <Vec<u8>>::from_hex(include_str!("block-test-0-000-006.txt").trim()).unwrap();
    /// Testnet block at height `7` (pre-overwinter).
    pub static ref BLOCK_TESTNET_0_000_007_BYTES: Vec<u8> =
        <Vec<u8>>::from_hex(include_str!("block-test-0-000-007.txt").trim()).unwrap();
    /// Testnet block at height `8` (pre-overwinter).
    pub static ref BLOCK_TESTNET_0_000_008_BYTES: Vec<u8> =
        <Vec<u8>>::from_hex(include_str!("block-test-0-000-008.txt").trim()).unwrap();
    /// Testnet block at height `9` (pre-overwinter).
    pub static ref BLOCK_TESTNET_0_000_009_BYTES: Vec<u8> =
        <Vec<u8>>::from_hex(include_str!("block-test-0-000-009.txt").trim()).unwrap();
    /// Testnet block at height `10` (pre-overwinter).
    pub static ref BLOCK_TESTNET_0_000_010_BYTES: Vec<u8> =
        <Vec<u8>>::from_hex(include_str!("block-test-0-000-010.txt").trim()).unwrap();
    /// Testnet block at height `207500` (first overwinter).
    pub static ref BLOCK_TESTNET_0_207_500_BYTES: Vec<u8> =
        <Vec<u8>>::from_hex(include_str!("block-test-0-207-500.txt").trim()).unwrap();
    /// Testnet block at height `280000` (first sapling).
    pub static ref BLOCK_TESTNET_0_280_000_BYTES: Vec<u8> =
        <Vec<u8>>::from_hex(include_str!("block-test-0-280-000.txt").trim()).unwrap();
    /// Testnet block at height `584000` (first blossom).
    pub static ref BLOCK_TESTNET_0_584_000_BYTES: Vec<u8> =
        <Vec<u8>>::from_hex(include_str!("block-test-0-584-000.txt").trim()).unwrap();
    /// Testnet block at height `903800` (first heartwood).
    pub static ref BLOCK_TESTNET_0_903_800_BYTES: Vec<u8> =
        <Vec<u8>>::from_hex(include_str!("block-test-0-903-800.txt").trim()).unwrap();
    /// Testnet block at height `1028500` (first canopy).
    pub static ref BLOCK_TESTNET_1_028_500_BYTES: Vec<u8> =
        <Vec<u8>>::from_hex(include_str!("block-test-1-028-500.txt").trim()).unwrap();
    /// Testnet block at height `1599199` (last canopy).
    pub static ref BLOCK_TESTNET_1_599_199_BYTES: Vec<u8> =
        <Vec<u8>>::from_hex(include_str!("block-test-1-599-199.txt").trim()).unwrap();
    /// Testnet block at height `1599200` (first nu5).
    pub static ref BLOCK_TESTNET_1_599_200_BYTES: Vec<u8> =
        <Vec<u8>>::from_hex(include_str!("block-test-1-599-200.txt").trim()).unwrap();
    /// Testnet block at height `1599200` (second nu5).
    pub static ref BLOCK_TESTNET_1_599_201_BYTES: Vec<u8> =
        <Vec<u8>>::from_hex(include_str!("block-test-1-599-201.txt").trim()).unwrap();
}
