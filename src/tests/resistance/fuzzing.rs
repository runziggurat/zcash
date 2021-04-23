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

use rand::{distributions::Standard, thread_rng, Rng};

use std::time::Duration;

const ITERATIONS: usize = 1000;

#[tokio::test]
async fn fuzzing_zeroes_pre_handshake() {
    // ZG-RESISTANCE-001
    //
    // zebra: sends a version before disconnecting.
    // zcashd: tbd.

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
    // zcashd: tbd.

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
    // zebra: sends a version before disconnecting.
    // zcashd: tbd.

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

pub const MAX_MESSAGE_LEN: usize = 2 * 1024 * 1024;
pub const HEADER_LEN: usize = 24;

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

fn metadata_compliant_random_bytes(n: usize) -> Vec<(MessageHeader, Vec<u8>)> {
    // TODO: messages with other commands.
    use crate::protocol::message::VERSION_COMMAND;

    (0..n)
        .map(|_| {
            let random_len: usize = thread_rng().gen_range(1..(64 * 1024));
            let random_payload: Vec<u8> = (&mut thread_rng())
                .sample_iter(Standard)
                .take(random_len)
                .collect();

            let header = MessageHeader::new(VERSION_COMMAND, &random_payload);

            (header, random_payload)
        })
        .collect()
}

// Testing connection rejection (full on closed or just ignored messages think:
//
// Verifying closed connections is easy: keep reading the stream until connection is closed while ignoring all other messages.
// Verifying messages are just ignored is harder?
//
// Cases:
// - Closed stream -> read.
// - Ignored messages leading to closed stream -> read.
// - Ignored messages, stream stays open -> write ping/pong or try handshake.
