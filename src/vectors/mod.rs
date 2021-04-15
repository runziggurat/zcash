use hex::FromHex;
use lazy_static::lazy_static;

lazy_static! {
    pub static ref BLOCK_TESTNET_GENESIS_BYTES: Vec<u8> =
        <Vec<u8>>::from_hex(include_str!("block-test-0-000-000.txt").trim()).unwrap();
    pub static ref BLOCK_TESTNET_1_BYTES: Vec<u8> =
        <Vec<u8>>::from_hex(include_str!("block-test-0-000-001.txt").trim()).unwrap();
}
