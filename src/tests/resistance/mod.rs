mod corrupt_message;
mod fuzzing_corrupted_messages;
mod fuzzing_incorrect_length;
mod fuzzing_random_bytes;
mod fuzzing_stress;
mod zeroes;

use std::{
    net::{IpAddr, Ipv4Addr, SocketAddr},
    time::Duration,
};

use crate::protocol::{
    message::{constants::*, Message},
    payload::{
        block::{Headers, LocatorHashes},
        Addr, Inv, Nonce, Version,
    },
};

use rand::{
    prelude::{Rng, SeedableRng},
    thread_rng,
};
use rand_chacha::ChaCha8Rng;

const ITERATIONS: usize = 100;
const DISCONNECT_TIMEOUT: Duration = Duration::from_secs(5);

/// List of message commands which contain payload bytes
const COMMANDS_WITH_PAYLOADS: [[u8; 12]; 13] = [
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

// Returns a randomly seeded `ChaCha8Rng` instance.
fn seeded_rng() -> ChaCha8Rng {
    let mut seed: <ChaCha8Rng as SeedableRng>::Seed = Default::default();
    thread_rng().fill(&mut seed);

    // We print the seed for reproducability.
    println!("Seed for RNG: {:?}", seed);

    // Isn't cryptographically secure but adequate enough as a general source of seeded randomness.
    ChaCha8Rng::from_seed(seed)
}

// Returns a random u32 which isn't the supplied value.
fn random_non_valid_u32(rng: &mut ChaCha8Rng, value: u32) -> u32 {
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
fn default_fuzz_messages() -> Vec<Message> {
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
