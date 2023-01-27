//! Useful helper functions for fuzzing.

use std::{
    convert::TryInto,
    net::{IpAddr, Ipv4Addr, SocketAddr},
};

use bytes::BufMut;
use rand::{
    distributions::Standard,
    prelude::{Rng, SeedableRng, SliceRandom},
    thread_rng,
};
use rand_chacha::ChaCha8Rng;

use crate::protocol::{
    message::{constants::*, Message, MessageHeader},
    payload::{
        block::{Headers, LocatorHashes},
        codec::Codec,
        Addr, Inv, Nonce, Version,
    },
};

/// A list of message commands which contain payload bytes.
pub const COMMANDS_WITH_PAYLOADS: [[u8; 12]; 13] = [
    VERSION_COMMAND,
    PING_COMMAND,
    PONG_COMMAND,
    ADDR_COMMAND,
    GETHEADERS_COMMAND,
    HEADERS_COMMAND,
    GETBLOCKS_COMMAND,
    BLOCK_COMMAND,
    GETDATA_COMMAND,
    INV_COMMAND,
    NOTFOUND_COMMAND,
    TX_COMMAND,
    REJECT_COMMAND,
];

const CORRUPTION_PROBABILITY: f64 = 0.5;

/// Returns a randomly seeded `ChaCha8Rng` instance, useful for making tests reproducible.
pub fn seeded_rng() -> ChaCha8Rng {
    let mut seed: <ChaCha8Rng as SeedableRng>::Seed = Default::default();
    thread_rng().fill(&mut seed);

    // We print the seed for reproducibility.
    println!("Seed for RNG: {seed:?}");

    // Isn't cryptographically secure but adequate enough as a general source of seeded randomness.
    ChaCha8Rng::from_seed(seed)
}

/// Returns the set of messages used for fuzz-testing.
/// This notably excludes [`Message::Version`] because it is
/// usually tested separately.
pub fn default_fuzz_messages() -> Vec<Message> {
    vec![
        Message::Version(Version::new(
            SocketAddr::new(IpAddr::V4(Ipv4Addr::LOCALHOST), 0),
            SocketAddr::new(IpAddr::V4(Ipv4Addr::LOCALHOST), 0),
        )),
        Message::MemPool,
        Message::Verack,
        Message::Ping(Nonce::default()),
        Message::Pong(Nonce::default()),
        Message::GetAddr,
        Message::Addr(Addr::empty()),
        Message::Headers(Headers::empty()),
        Message::GetHeaders(LocatorHashes::empty()),
        Message::GetBlocks(LocatorHashes::empty()),
        Message::GetData(Inv::empty()),
        Message::Inv(Inv::empty()),
        Message::NotFound(Inv::empty()),
    ]
}

/// Returns `n` random length sets of zeroes.
pub fn zeroes(rng: &mut ChaCha8Rng, n: usize) -> Vec<Vec<u8>> {
    (0..n)
        .map(|_| {
            let random_len: usize = rng.gen_range(1..(MAX_MESSAGE_LEN * 2));
            vec![0u8; random_len]
        })
        .collect()
}

/// Returns `n` random length sets of random bytes.
pub fn random_bytes(rng: &mut ChaCha8Rng, n: usize) -> Vec<Vec<u8>> {
    (0..n)
        .map(|_| {
            let random_len: usize = rng.gen_range(1..(64 * 1024));
            let random_payload: Vec<u8> = rng.sample_iter(Standard).take(random_len).collect();

            random_payload
        })
        .collect()
}

/// Returns a message with a valid header and payload of random bytes.
pub fn metadata_compliant_random_bytes(
    rng: &mut ChaCha8Rng,
    n: usize,
    commands: &[[u8; 12]],
) -> Vec<Vec<u8>> {
    (0..n)
        .map(|_| {
            let random_len: usize = rng.gen_range(1..(64 * 1024));
            let mut random_payload: Vec<u8> = rng.sample_iter(Standard).take(random_len).collect();

            let command = commands.choose(rng).unwrap();
            let header = MessageHeader::new(*command, &random_payload);

            let mut buffer = Vec::with_capacity(HEADER_LEN + random_payload.len());
            header.encode(&mut buffer).unwrap();
            buffer.append(&mut random_payload);

            buffer
        })
        .collect()
}

/// Corrupts `n` messages from the supplied set by replacing a random number of bytes with random bytes.
pub fn encode_slightly_corrupted_messages(
    rng: &mut ChaCha8Rng,
    n: usize,
    messages: &[Message],
) -> Vec<Vec<u8>> {
    (0..n)
        .map(|_| {
            let message = messages.choose(rng).unwrap();
            corrupt_message(rng, message)
        })
        .collect()
}

fn corrupt_message(rng: &mut ChaCha8Rng, message: &Message) -> Vec<u8> {
    let mut bytes = Default::default();
    message.encode(&mut bytes).unwrap();
    let vec: Vec<_> = bytes.to_vec();
    let (valid_header, valid_message) = vec.split_at(HEADER_LEN);

    let mut corrupted_header = corrupt_bytes(rng, valid_header);
    let corrupted_message = corrupt_bytes(rng, valid_message);

    corrupted_header.extend_from_slice(&corrupted_message);

    // Contains header + message.
    corrupted_header
}

fn corrupt_bytes(rng: &mut ChaCha8Rng, serialized: &[u8]) -> Vec<u8> {
    serialized
        .iter()
        .map(|byte| {
            if rng.gen_bool(CORRUPTION_PROBABILITY) {
                rng.gen()
            } else {
                *byte
            }
        })
        .collect()
}

/// Encodes a message and corrupts the body length bytes.
pub fn encode_message_with_corrupt_body_length(rng: &mut ChaCha8Rng, message: &Message) -> Vec<u8> {
    let mut bytes = Default::default();
    message.encode(&mut bytes).unwrap();
    let mut vec: Vec<_> = bytes.to_vec();

    let invalid_body_length = random_non_valid_u32(rng, (vec.len() - HEADER_LEN) as u32);
    (&mut vec[MAGIC_LEN + COMMAND_LEN..][..4]).put_u32_le(invalid_body_length);

    vec
}

/// Encodes a message and corrupts the checksum bytes.
pub fn encode_message_with_corrupt_checksum(rng: &mut ChaCha8Rng, message: &Message) -> Vec<u8> {
    let mut bytes = Default::default();
    message.encode(&mut bytes).unwrap();
    let mut vec: Vec<_> = bytes.to_vec();

    let offset = MAGIC_LEN + COMMAND_LEN + 4; // 4 = sizeof MessageHeader.body_length
    let valid_checksum = u32::from_le_bytes(vec[offset..][..4].try_into().unwrap());
    let invalid_checksum = random_non_valid_u32(rng, valid_checksum);
    (&mut vec[offset..][..4]).put_u32_le(invalid_checksum);

    vec
}

/// Returns a random u32 which isn't the supplied value.
fn random_non_valid_u32(rng: &mut ChaCha8Rng, value: u32) -> u32 {
    // Make sure the generated value isn't the same.
    let random_value = rng.gen();
    if value != random_value {
        random_value
    } else {
        random_value + 1
    }
}

/// Picks `n` random messages from `message_pool`, encodes them and corrupts the body length bytes.
pub fn encode_messages_with_corrupt_body_length(
    rng: &mut ChaCha8Rng,
    n: usize,
    message_pool: &[Message],
) -> Vec<Vec<u8>> {
    (0..n)
        .map(|_| {
            let message = message_pool.choose(rng).unwrap();

            encode_message_with_corrupt_body_length(rng, message)
        })
        .collect()
}

/// Picks `n` random messages from `message_pool`, encodes them and corrupts the checksum bytes.
pub fn encode_messages_with_corrupt_checksum(
    rng: &mut ChaCha8Rng,
    n: usize,
    message_pool: &[Message],
) -> Vec<Vec<u8>> {
    (0..n)
        .map(|_| {
            let message = message_pool.choose(rng).unwrap();

            encode_message_with_corrupt_checksum(rng, message)
        })
        .collect()
}
