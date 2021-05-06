// Messages to be tested:
// - Messages with any length and any content (random bytes).
// - Messages with plausible lengths, e.g. 24 bytes for header and within the expected range for the body.
// - Metadata-compliant messages, e.g. correct header, random body.
// - Slightly corrupted but otherwise valid messages, e.g. N% of body replaced with random bytes.
// - Messages with an incorrect checksum.
// - Messages with differing announced and actual lengths.
//
// A note on RNGs: each test makes use of one seeded ChaCha8Rng. If the test fails, the test
// can be rerun with the same seed to reproduce the failure. The seed is outputed in the case of a
// failure or can be inspected with `-- --nocapture`.

use crate::{
    helpers::{autorespond_and_expect_disconnect, initiate_handshake, initiate_version_exchange},
    protocol::{
        message::*,
        payload::{block::Headers, Addr, Nonce, Version},
    },
    setup::{
        config::read_config_file,
        node::{Action, Node},
    },
};

use rand::{
    distributions::Standard,
    prelude::{Rng, SeedableRng, SliceRandom},
    thread_rng,
};
use rand_chacha::ChaCha8Rng;
use tokio::{io::AsyncWriteExt, net::TcpStream};

const ITERATIONS: usize = 100;
const CORRUPTION_PROBABILITY: f64 = 0.5;

#[tokio::test]
async fn fuzzing_zeroes_pre_handshake() {
    // ZG-RESISTANCE-001 (part 1)
    //
    // zebra: sends a version before disconnecting.
    // zcashd: disconnects immediately (log: `INFO main: PROCESSMESSAGE: INVALID MESSAGESTART peer=1`).

    let mut rng = seeded_rng();
    let payloads = zeroes(&mut rng, ITERATIONS);

    let (zig, node_meta) = read_config_file();

    let mut node = Node::new(node_meta);
    node.initial_action(Action::WaitForConnection(zig.new_local_addr()))
        .start()
        .await;

    for payload in payloads {
        let mut peer_stream = TcpStream::connect(node.addr()).await.unwrap();
        let _ = peer_stream.write_all(&payload).await;

        autorespond_and_expect_disconnect(&mut peer_stream).await;
    }

    node.stop().await;
}

#[tokio::test]
async fn fuzzing_zeroes_during_handshake_responder_side() {
    // ZG-RESISTANCE-002 (part 1)
    //
    // zebra: responds with verack before disconnecting.
    // zcashd: disconnects immediately.

    let mut rng = seeded_rng();
    let payloads = zeroes(&mut rng, ITERATIONS);

    let (zig, node_meta) = read_config_file();

    let mut node = Node::new(node_meta);
    node.initial_action(Action::WaitForConnection(zig.new_local_addr()))
        .start()
        .await;

    for payload in payloads {
        let mut peer_stream = initiate_version_exchange(node.addr()).await.unwrap();

        // Write zeroes in place of Verack.
        let _ = peer_stream.write_all(&payload).await;

        autorespond_and_expect_disconnect(&mut peer_stream).await;
    }

    node.stop().await;
}

#[tokio::test]
async fn fuzzing_zeroes_post_handshake() {
    // ZG-RESISTANCE-005 (part 1)
    //
    // zebra: disconnects.
    // zcashd: responds with ping and getheaders before disconnecting.

    let mut rng = seeded_rng();
    let payloads = zeroes(&mut rng, ITERATIONS);

    let (zig, node_meta) = read_config_file();

    let mut node = Node::new(node_meta);
    node.start_waits_for_connection(zig.new_local_addr())
        .start()
        .await;

    for payload in payloads {
        let mut peer_stream = initiate_handshake(node.addr()).await.unwrap();

        // Write zeroes in place of Verack.
        let _ = peer_stream.write_all(&payload).await;

        autorespond_and_expect_disconnect(&mut peer_stream).await;
    }

    node.stop().await;
}

#[tokio::test]
async fn fuzzing_random_bytes_pre_handshake() {
    // ZG-RESISTANCE-001 (part 2)
    //
    // zebra: sends a version before disconnecting.
    // zcashd: ignores the bytes and disconnects.

    let mut rng = seeded_rng();
    let payloads = random_bytes(&mut rng, ITERATIONS);

    let (zig, node_meta) = read_config_file();

    let mut node = Node::new(node_meta);
    node.initial_action(Action::WaitForConnection(zig.new_local_addr()))
        .start()
        .await;

    for payload in payloads {
        let mut peer_stream = TcpStream::connect(node.addr()).await.unwrap();
        let _ = peer_stream.write_all(&payload).await;

        autorespond_and_expect_disconnect(&mut peer_stream).await;
    }

    node.stop().await;
}

#[tokio::test]
async fn fuzzing_random_bytes_during_handshake_responder_side() {
    // ZG-RESISTANCE-002 (part 2)
    //
    // zebra: responds with verack before disconnecting.
    // zcashd: responds with verack, pong and getheaders before disconnecting.

    let mut rng = seeded_rng();
    let payloads = random_bytes(&mut rng, ITERATIONS);

    let (zig, node_meta) = read_config_file();

    let mut node = Node::new(node_meta);
    node.initial_action(Action::WaitForConnection(zig.new_local_addr()))
        .start()
        .await;

    for payload in payloads {
        let mut peer_stream = initiate_version_exchange(node.addr()).await.unwrap();

        // Write random bytes in place of Verack.
        let _ = peer_stream.write_all(&payload).await;

        autorespond_and_expect_disconnect(&mut peer_stream).await;
    }

    node.stop().await;
}

#[tokio::test]
async fn fuzzing_metadata_compliant_random_bytes_pre_handshake() {
    // ZG-RESISTANCE-001 (part 3)
    //
    // zebra: breaks with a version command in header, otherwise sends verack before closing the
    // connection.
    // zcashd: just ignores the message and doesn't disconnect.

    // Payloadless messages are omitted.
    let commands = vec![
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

    let mut rng = seeded_rng();
    let payloads = metadata_compliant_random_bytes(&mut rng, ITERATIONS, commands);

    let (zig, node_meta) = read_config_file();

    let mut node = Node::new(node_meta);
    node.initial_action(Action::WaitForConnection(zig.new_local_addr()))
        .start()
        .await;

    for (header, payload) in payloads {
        let mut peer_stream = TcpStream::connect(node.addr()).await.unwrap();
        let _ = header.write_to_stream(&mut peer_stream).await;
        let _ = peer_stream.write_all(&payload).await;

        autorespond_and_expect_disconnect(&mut peer_stream).await;
    }

    node.stop().await;
}

#[tokio::test]
async fn fuzzing_metadata_compliant_random_bytes_during_handshake_responder_side() {
    // ZG-RESISTANCE-002 (part 3)
    //
    // zebra: breaks with a version command in header, otherwise sends verack before closing the
    // connection.
    // zcashd: responds with reject, ccode malformed and doesn't disconnect.

    // Payloadless messages are omitted.
    let commands = vec![
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

    let mut rng = seeded_rng();
    let payloads = metadata_compliant_random_bytes(&mut rng, ITERATIONS, commands);

    let (zig, node_meta) = read_config_file();

    let mut node = Node::new(node_meta);
    node.initial_action(Action::WaitForConnection(zig.new_local_addr()))
        .start()
        .await;

    for (header, payload) in payloads {
        let mut peer_stream = initiate_version_exchange(node.addr()).await.unwrap();

        // Write random bytes in place of Verack.
        let _ = header.write_to_stream(&mut peer_stream).await;
        let _ = peer_stream.write_all(&payload).await;

        autorespond_and_expect_disconnect(&mut peer_stream).await;
    }

    node.stop().await;
}

#[tokio::test]
async fn fuzzing_slightly_corrupted_version_pre_handshake() {
    // ZG-RESISTANCE-001 (part 4)
    //
    // This particular case is considered alone because it is at particular risk of causing
    // troublesome behaviour, as seen with the valid metadata fuzzing against zebra.
    //
    // zebra: sends a version before disconnecting.
    // zcashd: ignores message but doesn't disconnect.
    // (log ex:
    // `INFO main: PROCESSMESSAGE: INVALID MESSAGESTART ;ersion peer=3`
    // `INFO main: ProcessMessages(version, 86 bytes): CHECKSUM ERROR nChecksum=67412de1 hdr.nChecksum=ddca6880`
    // which indicates the message was recognised as invalid).

    let mut rng = seeded_rng();
    let (zig, node_meta) = read_config_file();

    let mut node = Node::new(node_meta);
    node.initial_action(Action::WaitForConnection(zig.new_local_addr()))
        .start()
        .await;

    for _ in 0..ITERATIONS {
        let mut peer_stream = TcpStream::connect(node.addr()).await.unwrap();
        let version =
            Message::Version(Version::new(node.addr(), peer_stream.local_addr().unwrap()));
        let corrupted_version = corrupt_message(&mut rng, &version);

        // Send corrupt Version in place of Verack.
        // Contains header + message.
        let _ = peer_stream.write_all(&corrupted_version).await;

        autorespond_and_expect_disconnect(&mut peer_stream).await;
    }

    node.stop().await;
}

#[tokio::test]
async fn fuzzing_slightly_corrupted_version_during_handshake_responder_side() {
    // ZG-RESISTANCE-002 (part 4)
    //
    // This particular case is considered alone because it is at particular risk of causing
    // troublesome behaviour, as seen with the valid metadata fuzzing against zebra.
    //
    // zebra: sends a verack before disconnecting (though somewhat slow running).
    // zcashd: logs suggest the message was ignored but the node doesn't disconnect.

    let mut rng = seeded_rng();
    let (zig, node_meta) = read_config_file();

    let mut node = Node::new(node_meta);
    node.initial_action(Action::WaitForConnection(zig.new_local_addr()))
        .start()
        .await;

    for _ in 0..ITERATIONS {
        let mut peer_stream = initiate_version_exchange(node.addr()).await.unwrap();

        let version_to_corrupt =
            Message::Version(Version::new(node.addr(), peer_stream.local_addr().unwrap()));
        let corrupted_version = corrupt_message(&mut rng, &version_to_corrupt);

        // Send corrupt Version in place of Verack.
        // Contains header + message.
        let _ = peer_stream.write_all(&corrupted_version).await;

        autorespond_and_expect_disconnect(&mut peer_stream).await;
    }

    node.stop().await;
}

#[tokio::test]
async fn fuzzing_slightly_corrupted_messages_pre_handshake() {
    // ZG-RESISTANCE-001 (part 4)
    //
    // zebra: responds with a version before disconnecting (however, quite slow running).
    // zcashd: just ignores the message and doesn't disconnect.

    let test_messages = vec![
        Message::MemPool,
        Message::Verack,
        Message::Ping(Nonce::default()),
        Message::Pong(Nonce::default()),
        Message::GetAddr,
        Message::Addr(Addr::empty()),
        Message::Headers(Headers::empty()),
        // Message::GetHeaders(LocatorHashes)),
        // Message::GetBlocks(LocatorHashes)),
        // Message::GetData(Inv));
        // Message::Inv(Inv));
        // Message::NotFound(Inv));
    ];

    let mut rng = seeded_rng();
    let payloads = slightly_corrupted_messages(&mut rng, ITERATIONS, test_messages);

    let (zig, node_meta) = read_config_file();

    let mut node = Node::new(node_meta);
    node.initial_action(Action::WaitForConnection(zig.new_local_addr()))
        .start()
        .await;

    for payload in payloads {
        let mut peer_stream = TcpStream::connect(node.addr()).await.unwrap();
        let _ = peer_stream.write_all(&payload).await;

        autorespond_and_expect_disconnect(&mut peer_stream).await;
    }

    node.stop().await;
}

#[tokio::test]
async fn fuzzing_slightly_corrupted_messages_during_handshake_responder_side() {
    // ZG-RESISTANCE-002 (part 4)
    //
    // zebra: responds with verack before disconnecting (however, quite slow running).
    // zcashd: logs suggest the messages were ignored, doesn't disconnect.

    let test_messages = vec![
        Message::MemPool,
        Message::Verack,
        Message::Ping(Nonce::default()),
        Message::Pong(Nonce::default()),
        Message::GetAddr,
        Message::Addr(Addr::empty()),
        Message::Headers(Headers::empty()),
        // Message::GetHeaders(LocatorHashes)),
        // Message::GetBlocks(LocatorHashes)),
        // Message::GetData(Inv));
        // Message::Inv(Inv));
        // Message::NotFound(Inv));
    ];

    let mut rng = seeded_rng();
    let payloads = slightly_corrupted_messages(&mut rng, ITERATIONS, test_messages);

    let (zig, node_meta) = read_config_file();

    let mut node = Node::new(node_meta);
    node.initial_action(Action::WaitForConnection(zig.new_local_addr()))
        .start()
        .await;

    for payload in payloads {
        let mut peer_stream = initiate_version_exchange(node.addr()).await.unwrap();

        // Write the corrupted message in place of Verack.
        let _ = peer_stream.write_all(&payload).await;

        autorespond_and_expect_disconnect(&mut peer_stream).await;
    }

    node.stop().await;
}

#[tokio::test]
async fn fuzzing_version_with_incorrect_checksum_pre_handshake() {
    // ZG-RESISTANCE-001 (part 5)
    //
    // zebra: sends version before disconnecting.
    // zcashd: log suggests messages was ignored, doesn't disconnect.

    let mut rng = seeded_rng();
    let (zig, node_meta) = read_config_file();

    let mut node = Node::new(node_meta);
    node.initial_action(Action::WaitForConnection(zig.new_local_addr()))
        .start()
        .await;

    for _ in 0..ITERATIONS {
        let mut peer_stream = TcpStream::connect(node.addr()).await.unwrap();

        let version =
            Message::Version(Version::new(node.addr(), peer_stream.local_addr().unwrap()));
        let mut message_buffer = vec![];
        let mut header = version.encode(&mut message_buffer).unwrap();

        // Set the checksum to a random value which isn't the current value.
        header.checksum = random_non_valid_u32(&mut rng, header.checksum);

        let _ = header.write_to_stream(&mut peer_stream).await;
        let _ = peer_stream.write_all(&message_buffer).await;

        autorespond_and_expect_disconnect(&mut peer_stream).await;
    }

    node.stop().await;
}

#[tokio::test]
async fn fuzzing_incorrect_checksum_pre_handshake() {
    // ZG-RESISTANCE-001 (part 5)
    //
    // zebra: sends a version before disconnecting.
    // zcashd: ignores the messages but doesn't disconnect (logs show a `CHECKSUM ERROR`).

    let mut rng = seeded_rng();
    let (zig, node_meta) = read_config_file();

    let mut node = Node::new(node_meta);
    node.initial_action(Action::WaitForConnection(zig.new_local_addr()))
        .start()
        .await;

    let test_messages = vec![
        Message::MemPool,
        Message::Verack,
        Message::Ping(Nonce::default()),
        Message::Pong(Nonce::default()),
        Message::GetAddr,
        Message::Addr(Addr::empty()),
        Message::Headers(Headers::empty()),
        // Message::GetHeaders(LocatorHashes)),
        // Message::GetBlocks(LocatorHashes)),
        // Message::GetData(Inv));
        // Message::Inv(Inv));
        // Message::NotFound(Inv));
    ];

    for _ in 0..ITERATIONS {
        let message = test_messages.choose(&mut rng).unwrap();
        let mut message_buffer = vec![];
        let mut header = message.encode(&mut message_buffer).unwrap();

        // Set the checksum to a random value which isn't the current value.
        header.checksum = random_non_valid_u32(&mut rng, header.checksum);

        let mut peer_stream = TcpStream::connect(node.addr()).await.unwrap();
        let _ = header.write_to_stream(&mut peer_stream).await;
        let _ = peer_stream.write_all(&message_buffer).await;

        autorespond_and_expect_disconnect(&mut peer_stream).await;
    }

    node.stop().await;
}

#[tokio::test]
async fn fuzzing_version_with_incorrect_checksum_during_handshake_responder_side() {
    // ZG-RESISTANCE-002 (part 5)
    //
    // zebra: sends verack before disconnecting.
    // zcashd: log suggests messages was ignored, sends verack, ping, getheaders but doesn't
    // disconnect.

    let mut rng = seeded_rng();
    let (zig, node_meta) = read_config_file();

    let mut node = Node::new(node_meta);
    node.initial_action(Action::WaitForConnection(zig.new_local_addr()))
        .start()
        .await;

    for _ in 0..ITERATIONS {
        let mut peer_stream = initiate_version_exchange(node.addr()).await.unwrap();

        let version =
            Message::Version(Version::new(node.addr(), peer_stream.local_addr().unwrap()));
        let mut message_buffer = vec![];
        let mut header = version.encode(&mut message_buffer).unwrap();

        // Set the checksum to a random value which isn't the current value.
        header.checksum = random_non_valid_u32(&mut rng, header.checksum);

        let _ = header.write_to_stream(&mut peer_stream).await;
        let _ = peer_stream.write_all(&message_buffer).await;

        autorespond_and_expect_disconnect(&mut peer_stream).await;
    }

    node.stop().await;
}

#[tokio::test]
async fn fuzzing_incorrect_checksum_during_handshake_responder_side() {
    // ZG-RESISTANCE-002 (part 5)
    //
    // zebra: sends a verack before disconnecting.
    // zcashd: logs indicate message was ignored, doesn't disconnect.

    let mut rng = seeded_rng();
    let (zig, node_meta) = read_config_file();

    let mut node = Node::new(node_meta);
    node.initial_action(Action::WaitForConnection(zig.new_local_addr()))
        .start()
        .await;

    let test_messages = vec![
        Message::MemPool,
        Message::Verack,
        Message::Ping(Nonce::default()),
        Message::Pong(Nonce::default()),
        Message::GetAddr,
        Message::Addr(Addr::empty()),
        Message::Headers(Headers::empty()),
        // Message::GetHeaders(LocatorHashes)),
        // Message::GetBlocks(LocatorHashes)),
        // Message::GetData(Inv));
        // Message::Inv(Inv));
        // Message::NotFound(Inv));
    ];

    for _ in 0..ITERATIONS {
        let message = test_messages.choose(&mut rng).unwrap();
        let mut message_buffer = vec![];
        let mut header = message.encode(&mut message_buffer).unwrap();

        // Set the checksum to a random value which isn't the current value.
        header.checksum = random_non_valid_u32(&mut rng, header.checksum);

        let mut peer_stream = initiate_version_exchange(node.addr()).await.unwrap();

        // Write messages with wrong checksum.
        let _ = header.write_to_stream(&mut peer_stream).await;
        let _ = peer_stream.write_all(&message_buffer).await;

        autorespond_and_expect_disconnect(&mut peer_stream).await;
    }

    node.stop().await;
}

#[tokio::test]
async fn fuzzing_version_with_incorrect_length_pre_handshake() {
    // ZG-RESISTANCE-001 (part 6)
    //
    // zebra: sends version before disconnecting.
    // zcashd: disconnects.

    let mut rng = seeded_rng();
    let (zig, node_meta) = read_config_file();

    let mut node = Node::new(node_meta);
    node.initial_action(Action::WaitForConnection(zig.new_local_addr()))
        .start()
        .await;

    for _ in 0..ITERATIONS {
        let mut peer_stream = TcpStream::connect(node.addr()).await.unwrap();

        let version =
            Message::Version(Version::new(node.addr(), peer_stream.local_addr().unwrap()));
        let mut message_buffer = vec![];
        let mut header = version.encode(&mut message_buffer).unwrap();

        // Set the length to a random value which isn't the current value.
        header.body_length = random_non_valid_u32(&mut rng, header.body_length);

        let _ = header.write_to_stream(&mut peer_stream).await;
        let _ = peer_stream.write_all(&message_buffer).await;

        autorespond_and_expect_disconnect(&mut peer_stream).await;
    }

    node.stop().await;
}

#[tokio::test]
async fn fuzzing_incorrect_length_pre_handshake() {
    // ZG-RESISTANCE-001 (part 6)
    //
    // zebra: disconnects.
    // zcashd: disconnects.

    let mut rng = seeded_rng();
    let (zig, node_meta) = read_config_file();

    let mut node = Node::new(node_meta);
    node.initial_action(Action::WaitForConnection(zig.new_local_addr()))
        .start()
        .await;

    let test_messages = vec![
        Message::GetAddr,
        Message::MemPool,
        Message::Verack,
        Message::Ping(Nonce::default()),
        Message::Pong(Nonce::default()),
        Message::GetAddr,
        Message::Addr(Addr::empty()),
        Message::Headers(Headers::empty()),
        // Message::GetHeaders(LocatorHashes)),
        // Message::GetBlocks(LocatorHashes)),
        // Message::GetData(Inv));
        // Message::Inv(Inv));
        // Message::NotFound(Inv));
    ];

    for _ in 0..ITERATIONS {
        let message = test_messages.choose(&mut rng).unwrap();
        let mut message_buffer = vec![];
        let mut header = message.encode(&mut message_buffer).unwrap();

        // Set the length to a random value which isn't the current value.
        header.body_length = random_non_valid_u32(&mut rng, header.body_length);

        let mut peer_stream = TcpStream::connect(node.addr()).await.unwrap();
        let _ = header.write_to_stream(&mut peer_stream).await;
        let _ = peer_stream.write_all(&message_buffer).await;

        autorespond_and_expect_disconnect(&mut peer_stream).await;
    }

    node.stop().await;
}

#[tokio::test]
async fn fuzzing_version_with_incorrect_length_during_handshake_responder_side() {
    // ZG-RESISTANCE-002 (part 6)
    //
    // zebra: sends verack before disconnecting.
    // zcashd: disconnects (after sending verack, ping, getheaders).

    let mut rng = seeded_rng();
    let (zig, node_meta) = read_config_file();

    let mut node = Node::new(node_meta);
    node.initial_action(Action::WaitForConnection(zig.new_local_addr()))
        .start()
        .await;

    for _ in 0..ITERATIONS {
        let mut peer_stream = initiate_version_exchange(node.addr()).await.unwrap();

        let version =
            Message::Version(Version::new(node.addr(), peer_stream.local_addr().unwrap()));
        let mut message_buffer = vec![];
        let mut header = version.encode(&mut message_buffer).unwrap();

        // Set the length to a random value which isn't the current value.
        header.body_length = random_non_valid_u32(&mut rng, header.body_length);

        let _ = header.write_to_stream(&mut peer_stream).await;
        let _ = peer_stream.write_all(&message_buffer).await;

        autorespond_and_expect_disconnect(&mut peer_stream).await;
    }

    node.stop().await;
}

#[tokio::test]
async fn fuzzing_incorrect_length_during_handshake_responder_side() {
    // ZG-RESISTANCE-002 (part 6)
    //
    // zebra: disconnects.
    // zcashd: disconnects (after sending verack, ping, getheaders).

    let mut rng = seeded_rng();
    let (zig, node_meta) = read_config_file();

    let mut node = Node::new(node_meta);
    node.initial_action(Action::WaitForConnection(zig.new_local_addr()))
        .start()
        .await;

    let test_messages = vec![
        Message::MemPool,
        Message::Verack,
        Message::Ping(Nonce::default()),
        Message::Pong(Nonce::default()),
        Message::GetAddr,
        Message::Addr(Addr::empty()),
        Message::Headers(Headers::empty()),
        // Message::GetHeaders(LocatorHashes)),
        // Message::GetBlocks(LocatorHashes)),
        // Message::GetData(Inv));
        // Message::Inv(Inv));
        // Message::NotFound(Inv));
    ];

    for _ in 0..ITERATIONS {
        let message = test_messages.choose(&mut rng).unwrap();
        let mut message_buffer = vec![];
        let mut header = message.encode(&mut message_buffer).unwrap();

        // Set the length to a random value which isn't the current value.
        header.body_length = random_non_valid_u32(&mut rng, header.body_length);

        let mut peer_stream = initiate_version_exchange(node.addr()).await.unwrap();

        // Send message with wrong lenght in place of valid Verack.
        let _ = header.write_to_stream(&mut peer_stream).await;
        let _ = peer_stream.write_all(&message_buffer).await;

        autorespond_and_expect_disconnect(&mut peer_stream).await;
    }

    node.stop().await;
}

// Returns a randomly seeded `ChaCha8Rng` instance.
fn seeded_rng() -> ChaCha8Rng {
    let mut seed: <ChaCha8Rng as SeedableRng>::Seed = Default::default();
    thread_rng().fill(&mut seed);

    // We print the seed for reproducability.
    println!("Seed for RNG: {:?}", seed);

    // Isn't cryptographically secure but adequate enough as a general source of seeded randomness.
    ChaCha8Rng::from_seed(seed)
}

// Random length zeroes.
fn zeroes(rng: &mut ChaCha8Rng, n: usize) -> Vec<Vec<u8>> {
    (0..n)
        .map(|_| {
            let random_len: usize = rng.gen_range(1..(MAX_MESSAGE_LEN * 2));
            dbg!(random_len);
            vec![0u8; random_len]
        })
        .collect()
}

// Random length, random bytes.
fn random_bytes(rng: &mut ChaCha8Rng, n: usize) -> Vec<Vec<u8>> {
    (0..n)
        .map(|_| {
            let random_len: usize = rng.gen_range(1..(64 * 1024));
            let random_payload: Vec<u8> = rng.sample_iter(Standard).take(random_len).collect();

            random_payload
        })
        .collect()
}

// Valid message header, random bytes as message.
fn metadata_compliant_random_bytes(
    rng: &mut ChaCha8Rng,
    n: usize,
    commands: Vec<[u8; 12]>,
) -> Vec<(MessageHeader, Vec<u8>)> {
    (0..n)
        .map(|_| {
            let random_len: usize = rng.gen_range(1..(64 * 1024));
            let random_payload: Vec<u8> = rng.sample_iter(Standard).take(random_len).collect();

            let command = commands.choose(rng).unwrap();
            let header = MessageHeader::new(*command, &random_payload);

            (header, random_payload)
        })
        .collect()
}

// Corrupt messages from the supplied set by replacing a random number of bytes with random bytes.
fn slightly_corrupted_messages(
    rng: &mut ChaCha8Rng,
    n: usize,
    messages: Vec<Message>,
) -> Vec<Vec<u8>> {
    (0..n)
        .map(|_| {
            let message = messages.choose(rng).unwrap();
            corrupt_message(rng, &message)
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
