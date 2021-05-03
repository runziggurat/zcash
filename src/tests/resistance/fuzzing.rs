// Messages to be tested:
// - Messages with any length and any content (random bytes).
// - Messages with plausible lengths, e.g. 24 bytes for header and within the expected range for the body.
// - Metadata-compliant messages, e.g. correct header, random body.
// - Slightly corrupted but otherwise valid messages, e.g. N% of body replaced with random bytes.
// - Messages with an incorrect checksum.
// - Messages with differing announced and actual lengths.

use crate::{
    protocol::{
        message::{Filter, Message, MessageFilter, MessageHeader},
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
    // ZG-RESISTANCE-001
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

        let auto_responder = MessageFilter::with_all_auto_reply().enable_logging();

        for _ in 0usize..10 {
            let result = timeout(
                Duration::from_secs(5),
                auto_responder.read_from_stream(&mut peer_stream),
            )
            .await;

            match result {
                Err(elapsed) => panic!("Timeout after {}", elapsed),
                Ok(Ok(message)) => println!("Received unfiltered message: {:?}", message),
                Ok(Err(err)) => assert!(is_termination_error(&err)),
            }
        }
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

        let auto_responder = MessageFilter::with_all_auto_reply().enable_logging();

        for _ in 0usize..10 {
            let result = timeout(
                Duration::from_secs(5),
                auto_responder.read_from_stream(&mut peer_stream),
            )
            .await;

            match result {
                Err(elapsed) => panic!("Timeout after {}", elapsed),
                Ok(Ok(message)) => println!("Received unfiltered message: {:?}", message),
                Ok(Err(err)) => assert!(is_termination_error(&err)),
            }
        }
    }

    node.stop().await;
}

#[tokio::test]
async fn fuzzing_metadata_compliant_random_bytes_pre_handshake() {
    // ZG-RESISTANCE-001 (part 3)
    //
    // zebra: breaks with a version command in header.
    // zcashd: just ignores the message and doesn't disconnect.

    let payloads = metadata_compliant_random_bytes(ITERATIONS);

    let (zig, node_meta) = read_config_file();

    let mut node = Node::new(node_meta);
    node.start_waits_for_connection(zig.new_local_addr())
        .start()
        .await;

    for (header, payload) in payloads {
        let mut peer_stream = TcpStream::connect(node.addr()).await.unwrap();
        let _ = header.write_to_stream(&mut peer_stream).await;
        let _ = peer_stream.write_all(&payload).await;

        let auto_responder = MessageFilter::with_all_auto_reply().enable_logging();

        for _ in 0usize..10 {
            let result = timeout(
                Duration::from_secs(5),
                auto_responder.read_from_stream(&mut peer_stream),
            )
            .await;

            match result {
                Err(elapsed) => panic!("Timeout after {}", elapsed),
                Ok(Ok(message)) => println!("Received unfiltered message: {:?}", message),
                Ok(Err(err)) => assert!(is_termination_error(&err)),
            }
        }
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
    // zebra: disconnects as expected.
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

        let auto_responder = MessageFilter::with_all_auto_reply().enable_logging();

        for _ in 0usize..10 {
            let result = timeout(
                Duration::from_secs(5),
                auto_responder.read_from_stream(&mut peer_stream),
            )
            .await;

            // Expect a disconnect.
            match result {
                Err(elapsed) => panic!("Timeout after {}", elapsed),
                Ok(Ok(message)) => println!("Received unfiltered message: {:?}", message),
                Ok(Err(err)) => assert!(is_termination_error(&err)),
            }
        }
    }

    node.stop().await;
}

#[tokio::test]
async fn fuzzing_slightly_corrupted_messages_pre_handshake() {
    // ZG-RESISTANCE-001 (part 4)
    //
    // zebra: rejects messages and disconnects (however, quite slow running).
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

        let auto_responder = MessageFilter::with_all_auto_reply().enable_logging();

        for _ in 0usize..10 {
            let result = timeout(
                Duration::from_secs(5),
                auto_responder.read_from_stream(&mut peer_stream),
            )
            .await;

            match result {
                Err(elapsed) => panic!("Timeout after {}", elapsed),
                Ok(Ok(message)) => println!("Received unfiltered message: {:?}", message),
                Ok(Err(err)) => assert!(is_termination_error(&err)),
            }
        }
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

// Messages to be tested:
// - Messages with any length and any content (random bytes).
// - Messages with plausible lengths, e.g. 24 bytes for header and within the expected range for the body.
// - Metadata-compliant messages, e.g. correct header, random body.
// - Slightly corrupted but otherwise valid messages, e.g. N% of body replaced with random bytes.
// - Messages with an incorrect checksum.
// - Messages with differing announced and actual lengths.

fn zeroes(n: usize) -> Vec<Vec<u8>> {
    use crate::protocol::message::MAX_MESSAGE_LEN;
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

fn metadata_compliant_random_bytes(n: usize) -> Vec<(MessageHeader, Vec<u8>)> {
    use crate::protocol::message::*;

    let mut rng = thread_rng();

    (0..n)
        .map(|_| {
            let random_len: usize = rng.gen_range(1..(64 * 1024));
            let random_payload: Vec<u8> =
                (&mut rng).sample_iter(Standard).take(random_len).collect();

            let commands = [
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

// Testing connection rejection (closed or just ignored messages):
//
// Verifying closed connections is easy: keep reading the stream until connection is closed while ignoring all other messages.
// Verifying messages are just ignored is harder?
//
// Cases:
// - Closed stream -> read.
// - Ignored messages leading to closed stream -> read.
// - Ignored messages, stream stays open -> write ping/pong or try handshake.
