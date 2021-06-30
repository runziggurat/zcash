//! Useful helper functions for fuzzing.

use std::net::{IpAddr, Ipv4Addr, SocketAddr};

use crate::protocol::{
    message::{constants::*, Message, MessageHeader},
    payload::{
        block::{Headers, LocatorHashes},
        codec::Codec,
        Addr, Inv, Nonce, Version,
    },
};

use rand::{
    distributions::Standard,
    prelude::{Rng, SeedableRng, SliceRandom},
    thread_rng,
};
use rand_chacha::ChaCha8Rng;

/// List of message commands which contain payload bytes
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

// Returns a randomly seeded `ChaCha8Rng` instance.
pub fn seeded_rng() -> ChaCha8Rng {
    let mut seed: <ChaCha8Rng as SeedableRng>::Seed = Default::default();
    thread_rng().fill(&mut seed);

    // We print the seed for reproducability.
    println!("Seed for RNG: {:?}", seed);

    // Isn't cryptographically secure but adequate enough as a general source of seeded randomness.
    ChaCha8Rng::from_seed(seed)
}

// Returns a random u32 which isn't the supplied value.
pub fn random_non_valid_u32(rng: &mut ChaCha8Rng, value: u32) -> u32 {
    // Make sure the generated value isn't the same.
    let random_value = rng.gen();
    if value != random_value {
        random_value
    } else {
        random_value + 1
    }
}

/// Returns the set of messages used for fuzz-testing.
/// This notably excludes [Message::Version] because it is
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

// Random length zeroes.
pub fn zeroes(rng: &mut ChaCha8Rng, n: usize) -> Vec<Vec<u8>> {
    (0..n)
        .map(|_| {
            let random_len: usize = rng.gen_range(1..(MAX_MESSAGE_LEN * 2));
            vec![0u8; random_len]
        })
        .collect()
}

// Random length, random bytes.
pub fn random_bytes(rng: &mut ChaCha8Rng, n: usize) -> Vec<Vec<u8>> {
    (0..n)
        .map(|_| {
            let random_len: usize = rng.gen_range(1..(64 * 1024));
            let random_payload: Vec<u8> = rng.sample_iter(Standard).take(random_len).collect();

            random_payload
        })
        .collect()
}

// Corrupt messages from the supplied set by replacing a random number of bytes with random bytes.
pub fn slightly_corrupted_messages(
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
    let mut message_buffer = vec![];
    let header = message.encode(&mut message_buffer).unwrap();
    let mut header_buffer = vec![];
    header.encode(&mut header_buffer).unwrap();

    let mut corrupted_header = corrupt_bytes(rng, &header_buffer);
    let mut corrupted_message = corrupt_bytes(rng, &message_buffer);

    corrupted_header.append(&mut corrupted_message);

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

pub fn encode_with_corrupt_body_length(rng: &mut ChaCha8Rng, message: &Message) -> Vec<u8> {
    let mut body_buffer = Vec::new();
    let mut header = message.encode(&mut body_buffer).unwrap();

    let mut buffer = Vec::with_capacity(body_buffer.len() + HEADER_LEN);
    header.body_length = random_non_valid_u32(rng, header.body_length);
    header.encode(&mut buffer).unwrap();
    buffer.append(&mut body_buffer);

    buffer
}

/// Picks `n` random messages from `message_pool`, encodes them and corrupts the body-length field
pub fn encode_messages_and_corrupt_body_length_field(
    rng: &mut ChaCha8Rng,
    n: usize,
    message_pool: &[Message],
) -> Vec<Vec<u8>> {
    (0..n)
        .map(|_| {
            let message = message_pool.choose(rng).unwrap();

            encode_with_corrupt_body_length(rng, message)
        })
        .collect()
}

/// Picks `n` random messages from `message_pool`, encodes them and corrupts the checksum bytes.
pub fn encode_messages_and_corrupt_checksum(
    rng: &mut ChaCha8Rng,
    n: usize,
    message_pool: &[Message],
) -> Vec<Vec<u8>> {
    (0..n)
        .map(|_| {
            let message = message_pool.choose(rng).unwrap();

            let mut body_buffer = Vec::new();
            let mut header = message.encode(&mut body_buffer).unwrap();

            let mut buffer = Vec::with_capacity(body_buffer.len() + HEADER_LEN);
            header.checksum = random_non_valid_u32(rng, header.checksum);
            header.encode(&mut buffer).unwrap();
            buffer.append(&mut body_buffer);

            buffer
        })
        .collect()
}

// Valid message header, random bytes as message.
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
