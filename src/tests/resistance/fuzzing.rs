// Messages to be tested:
// - Messages with any length and any content (random bytes).
// - Messages with plausible lengths, e.g. 24 bytes for header and within the expected range for the body.
// - Metadata-compliant messages, e.g. correct header, random body.
// - Slightly corrupted but otherwise valid messages, e.g. N% of body replaced with random bytes.
// - Messages with an incorrect checksum.
// - Messages with differing announced and actual lengths.

// Testing connection rejection (closed or just ignored messages):
//
// Verifying closed connections is easy: keep reading the stream until connection is closed while ignoring all other messages.
// Verifying messages are just ignored is harder?
//
// Cases:
// - Closed stream -> read.
// - Ignored messages leading to closed stream -> read.
// - Ignored messages, stream stays open -> write ping/pong or try handshake.

use crate::{
    protocol::{
        message::*,
        payload::{block::Headers, Addr, Nonce, Version},
    },
    setup::{config::read_config_file, node::Node},
};

use tokio::{
    io::AsyncWriteExt,
    net::{TcpListener, TcpStream},
    time::timeout,
};

use rand::{distributions::Standard, prelude::SliceRandom, thread_rng, Rng};

use std::time::Duration;

const ITERATIONS: usize = 100;

#[tokio::test]
async fn fuzzing_zeroes_pre_handshake() {
    // ZG-RESISTANCE-001 (part 1)
    //
    // zebra: sends a version before disconnecting.
    // zcashd: disconnects immediately (log: `INFO main: PROCESSMESSAGE: INVALID MESSAGESTART peer=1`).

    let payloads = zeroes(ITERATIONS);

    let (zig, node_meta) = read_config_file();

    let mut node = Node::new(node_meta);
    node.start_waits_for_connection(zig.new_local_addr())
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

    let payloads = zeroes(ITERATIONS);

    let (zig, node_meta) = read_config_file();

    let mut node = Node::new(node_meta);
    node.start_waits_for_connection(zig.new_local_addr())
        .start()
        .await;

    for payload in payloads {
        let mut peer_stream = TcpStream::connect(node.addr()).await.unwrap();

        // Send and receive Version.
        Message::Version(Version::new(node.addr(), peer_stream.local_addr().unwrap()))
            .write_to_stream(&mut peer_stream)
            .await
            .unwrap();

        let version = Message::read_from_stream(&mut peer_stream).await.unwrap();
        assert!(matches!(version, Message::Version(..)));

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

    let payloads = random_bytes(ITERATIONS);

    let (zig, node_meta) = read_config_file();

    let mut node = Node::new(node_meta);
    node.start_waits_for_connection(zig.new_local_addr())
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

    let payloads = random_bytes(ITERATIONS);

    let (zig, node_meta) = read_config_file();

    let mut node = Node::new(node_meta);
    node.start_waits_for_connection(zig.new_local_addr())
        .start()
        .await;

    for payload in payloads {
        let mut peer_stream = TcpStream::connect(node.addr()).await.unwrap();

        // Send and receive Version.
        Message::Version(Version::new(node.addr(), peer_stream.local_addr().unwrap()))
            .write_to_stream(&mut peer_stream)
            .await
            .unwrap();

        let version = Message::read_from_stream(&mut peer_stream).await.unwrap();
        assert!(matches!(version, Message::Version(..)));

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

    let commands = vec![
        VERSION_COMMAND,
        VERACK_COMMAND,
        PING_COMMAND,
        PONG_COMMAND,
        GETADDR_COMMAND,
        ADDR_COMMAND,
        GETHEADERS_COMMAND,
        HEADERS_COMMAND,
        GETBLOCKS_COMMAND,
        BLOCK_COMMAND,
        GETDATA_COMMAND,
        INV_COMMAND,
        NOTFOUND_COMMAND,
        MEMPOOL_COMMAND,
        TX_COMMAND,
        REJECT_COMMAND,
    ];
    let payloads = metadata_compliant_random_bytes(ITERATIONS, commands);

    let (zig, node_meta) = read_config_file();

    let mut node = Node::new(node_meta);
    node.start_waits_for_connection(zig.new_local_addr())
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

    let commands = vec![
        VERSION_COMMAND,
        PING_COMMAND,
        PONG_COMMAND,
        GETADDR_COMMAND,
        ADDR_COMMAND,
        GETHEADERS_COMMAND,
        HEADERS_COMMAND,
        GETBLOCKS_COMMAND,
        BLOCK_COMMAND,
        GETDATA_COMMAND,
        INV_COMMAND,
        NOTFOUND_COMMAND,
        MEMPOOL_COMMAND,
        TX_COMMAND,
        REJECT_COMMAND,
    ];
    let payloads = metadata_compliant_random_bytes(ITERATIONS, commands);

    let (zig, node_meta) = read_config_file();

    let mut node = Node::new(node_meta);
    node.start_waits_for_connection(zig.new_local_addr())
        .start()
        .await;

    for (header, payload) in payloads {
        let mut peer_stream = TcpStream::connect(node.addr()).await.unwrap();

        // Send and receive Version.
        Message::Version(Version::new(node.addr(), peer_stream.local_addr().unwrap()))
            .write_to_stream(&mut peer_stream)
            .await
            .unwrap();

        let version = Message::read_from_stream(&mut peer_stream).await.unwrap();
        assert!(matches!(version, Message::Version(..)));

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

    let (zig, node_meta) = read_config_file();

    let mut node = Node::new(node_meta);
    node.start_waits_for_connection(zig.new_local_addr())
        .start()
        .await;

    for _ in 0..ITERATIONS {
        let mut peer_stream = TcpStream::connect(node.addr()).await.unwrap();
        let message =
            Message::Version(Version::new(node.addr(), peer_stream.local_addr().unwrap()));

        let mut message_buffer = vec![];
        let header = message.encode(&mut message_buffer).unwrap();
        let mut header_buffer = vec![];
        header.encode(&mut header_buffer).unwrap();

        let mut corrupted_header = corrupt_bytes(&header_buffer);
        let mut corrupted_message = corrupt_bytes(&message_buffer);

        corrupted_header.append(&mut corrupted_message);

        // Contains header + message.
        let _ = peer_stream.write_all(&corrupted_header).await;

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

    let payloads = slightly_corrupted_messages(ITERATIONS);

    let (zig, node_meta) = read_config_file();

    let mut node = Node::new(node_meta);
    node.start_waits_for_connection(zig.new_local_addr())
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
async fn fuzzing_incorrect_checksum_pre_handshake() {
    // ZG-RESISTANCE-001 (part 5)
    //
    // zebra: sends a version before disconnecting.
    // zcashd: ignores the messages but doesn't disconnect (logs show a `CHECKSUM ERROR`).

    let (zig, node_meta) = read_config_file();

    let mut node = Node::new(node_meta);
    node.start_waits_for_connection(zig.new_local_addr())
        .start()
        .await;

    let mut rng = thread_rng();

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

        // Change the checksum advertised in the header (last 4 bytes), make sure the randomly
        // generated checksum isn't the same as the valid one.
        let random_checksum = rng.gen();
        if header.checksum != random_checksum {
            header.checksum = random_checksum
        } else {
            header.checksum += 1;
        }

        let mut peer_stream = TcpStream::connect(node.addr()).await.unwrap();
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

    let (zig, node_meta) = read_config_file();

    let mut node = Node::new(node_meta);
    node.start_waits_for_connection(zig.new_local_addr())
        .start()
        .await;

    let mut rng = thread_rng();

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

        // Change the checksum advertised in the header (last 4 bytes), make sure the randomly
        // generated checksum isn't the same as the valid one.
        let random_len = rng.gen();
        if header.body_length != random_len {
            header.body_length = random_len
        } else {
            header.body_length += 1;
        }

        let mut peer_stream = TcpStream::connect(node.addr()).await.unwrap();
        let _ = header.write_to_stream(&mut peer_stream).await;
        let _ = peer_stream.write_all(&message_buffer).await;

        autorespond_and_expect_disconnect(&mut peer_stream).await;
    }

    node.stop().await;
}

// Returns true if the error kind is one that indicates that the connection has
// been terminated.
// TODO: dedup
fn is_termination_error(err: &std::io::Error) -> bool {
    use std::io::ErrorKind::*;
    matches!(
        err.kind(),
        ConnectionReset | ConnectionAborted | BrokenPipe | UnexpectedEof
    )
}

fn zeroes(n: usize) -> Vec<Vec<u8>> {
    // Random length zeroes.
    (0..n)
        .map(|_| {
            let random_len: usize = thread_rng().gen_range(1..(MAX_MESSAGE_LEN * 2));
            vec![0u8; random_len]
        })
        .collect()
}

fn random_bytes(n: usize) -> Vec<Vec<u8>> {
    (0..n)
        .map(|_| {
            let random_len: usize = thread_rng().gen_range(1..(64 * 1024));
            let random_payload: Vec<u8> = (&mut thread_rng())
                .sample_iter(Standard)
                .take(random_len)
                .collect();

            random_payload
        })
        .collect()
}

fn metadata_compliant_random_bytes(
    n: usize,
    commands: Vec<[u8; 12]>,
) -> Vec<(MessageHeader, Vec<u8>)> {
    let mut rng = thread_rng();

    (0..n)
        .map(|_| {
            let random_len: usize = rng.gen_range(1..(64 * 1024));
            let random_payload: Vec<u8> =
                (&mut rng).sample_iter(Standard).take(random_len).collect();

            let command = commands.choose(&mut rng).unwrap();
            let header = MessageHeader::new(*command, &random_payload);

            (header, random_payload)
        })
        .collect()
}

fn slightly_corrupted_messages(n: usize) -> Vec<Vec<u8>> {
    let mut rng = thread_rng();

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

    (0..n)
        .map(|_| {
            let message = test_messages.choose(&mut rng).unwrap();
            let mut message_buffer = vec![];
            let header = message.encode(&mut message_buffer).unwrap();
            let mut header_buffer = vec![];
            header.encode(&mut header_buffer).unwrap();

            let mut corrupted_header = corrupt_bytes(&header_buffer);
            let mut corrupted_message = corrupt_bytes(&message_buffer);

            corrupted_header.append(&mut corrupted_message);

            // Contains header + message.
            corrupted_header
        })
        .collect()
}

pub const CORRUPTION_PROBABILITY: f64 = 0.1;

fn corrupt_bytes(serialized: &[u8]) -> Vec<u8> {
    let mut rng = thread_rng();

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

async fn autorespond_and_expect_disconnect(stream: &mut TcpStream) {
    let auto_responder = MessageFilter::with_all_auto_reply().enable_logging();

    let mut is_disconnect = false;

    // Read a maximum of 10 messages before exiting.
    for _ in 0usize..10 {
        let result = timeout(
            Duration::from_secs(5),
            auto_responder.read_from_stream(stream),
        )
        .await;

        match result {
            Err(elapsed) => panic!("Timeout after {}", elapsed),
            Ok(Ok(message)) => println!("Received unfiltered message: {:?}", message),
            Ok(Err(err)) => {
                if is_termination_error(&err) {
                    is_disconnect = true;
                    break;
                }
            }
        }
    }

    assert!(is_disconnect);
}
