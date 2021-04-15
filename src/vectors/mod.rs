//! Test vectors ordered by block height.
//!
//! Please note, these vectors have been copied across from [zebra](https://github.com/ZcashFoundation/zebra/tree/main/zebra-test/src/vectors).
//!
//! Blocks index:
//!
//! Genesis -> 2: sequential pre-overwinter.
//! 207500: first overwinter.
//! 280000: first sapling.
//! 584000: first blossom.
//! 903800: first heartwood.
//! 1028500: first canopy.

use hex::FromHex;
use lazy_static::lazy_static;

lazy_static! {
    pub static ref BLOCK_TESTNET_GENESIS_BYTES: Vec<u8> =
        <Vec<u8>>::from_hex(include_str!("block-test-0-000-000.txt").trim()).unwrap();
    pub static ref BLOCK_TESTNET_1_BYTES: Vec<u8> =
        <Vec<u8>>::from_hex(include_str!("block-test-0-000-001.txt").trim()).unwrap();
    pub static ref BLOCK_TESTNET_2_BYTES: Vec<u8> =
        <Vec<u8>>::from_hex(include_str!("block-test-0-000-002.txt").trim()).unwrap();
    pub static ref BLOCK_TESTNET_207500_BYTES: Vec<u8> =
        <Vec<u8>>::from_hex(include_str!("block-test-0-207-500.txt").trim()).unwrap();
    pub static ref BLOCK_TESTNET_280000_BYTES: Vec<u8> =
        <Vec<u8>>::from_hex(include_str!("block-test-0-280-000.txt").trim()).unwrap();
    pub static ref BLOCK_TESTNET_584000_BYTES: Vec<u8> =
        <Vec<u8>>::from_hex(include_str!("block-test-0-584-000.txt").trim()).unwrap();
    pub static ref BLOCK_TESTNET_903800_BYTES: Vec<u8> =
        <Vec<u8>>::from_hex(include_str!("block-test-0-903-800.txt").trim()).unwrap();
    pub static ref BLOCK_TESTNET_1028500_BYTES: Vec<u8> =
        <Vec<u8>>::from_hex(include_str!("block-test-1-028-500.txt").trim()).unwrap();
}
