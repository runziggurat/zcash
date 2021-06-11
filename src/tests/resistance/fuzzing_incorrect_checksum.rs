use crate::{
    helpers::{autorespond_and_expect_disconnect, initiate_handshake, initiate_version_exchange},
    protocol::{
        message::{constants::HEADER_LEN, Message},
        payload::{codec::Codec, Version},
    },
    setup::{
        config::new_local_addr,
        node::{Action, Node},
    },
    tests::resistance::{default_fuzz_messages, random_non_valid_u32, seeded_rng, ITERATIONS},
};

use assert_matches::assert_matches;
use rand::prelude::SliceRandom;
use rand_chacha::ChaCha8Rng;
use tokio::{
    io::AsyncWriteExt,
    net::{TcpListener, TcpStream},
};

#[tokio::test]
async fn version_with_incorrect_checksum_pre_handshake() {
    // ZG-RESISTANCE-001 (part 5)
    //
    // zebra: sends version before disconnecting.
    // zcashd: log suggests messages was ignored, doesn't disconnect.

    let mut rng = seeded_rng();

    let mut node: Node = Default::default();
    node.initial_action(Action::WaitForConnection).start().await;

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
async fn incorrect_checksum_pre_handshake() {
    // ZG-RESISTANCE-001 (part 5)
    //
    // zebra: sends a version before disconnecting.
    // zcashd: ignores the messages but doesn't disconnect (logs show a `CHECKSUM ERROR`).

    let mut rng = seeded_rng();

    let mut node: Node = Default::default();
    node.initial_action(Action::WaitForConnection).start().await;

    let test_messages = default_fuzz_messages();

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
async fn version_with_incorrect_checksum_during_handshake_responder_side() {
    // ZG-RESISTANCE-002 (part 5)
    //
    // zebra: sends verack before disconnecting.
    // zcashd: log suggests messages was ignored, sends verack, ping, getheaders but doesn't
    // disconnect.

    let mut rng = seeded_rng();

    let mut node: Node = Default::default();
    node.initial_action(Action::WaitForConnection).start().await;

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
async fn version_with_incorrect_checksum_post_handshake() {
    // ZG-RESISTANCE-005 (part 5)
    //
    // zebra: disconnects.
    // zcashd: logs indicate message was ignored, no disconnect.

    let mut rng = seeded_rng();
    let mut node: Node = Default::default();
    node.initial_action(Action::WaitForConnection).start().await;

    for _ in 0..ITERATIONS {
        let mut peer_stream = initiate_handshake(node.addr()).await.unwrap();

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
async fn incorrect_checksum_during_handshake_responder_side() {
    // ZG-RESISTANCE-002 (part 5)
    //
    // zebra: sends a verack before disconnecting.
    // zcashd: logs indicate message was ignored, doesn't disconnect.

    let mut rng = seeded_rng();

    let mut node: Node = Default::default();
    node.initial_action(Action::WaitForConnection).start().await;

    let test_messages = default_fuzz_messages();

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
async fn incorrect_checksum_inplace_of_version_when_node_initiates_handshake() {
    // ZG-RESISTANCE-003 (part 5)
    //
    // zebra: disconnects immediately.
    // zcashd: Messages appear to get ignored

    let mut rng = seeded_rng();

    let test_messages = default_fuzz_messages();

    let mut payloads = encode_messages_and_corrupt_checksum(&mut rng, ITERATIONS, &test_messages);

    // create tcp listeners for peer set (port is only assigned on tcp bind)
    let mut listeners = Vec::with_capacity(ITERATIONS);
    for _ in 0..ITERATIONS {
        listeners.push(TcpListener::bind(new_local_addr()).await.unwrap());
    }

    // get list of peer addresses to pass to node
    let peer_addresses = listeners
        .iter()
        .map(|listener| listener.local_addr().unwrap())
        .collect::<Vec<_>>();

    // start peer processes
    let mut peer_handles = Vec::with_capacity(listeners.len());
    for peer in listeners {
        let payload = payloads.pop().unwrap();

        peer_handles.push(tokio::time::timeout(
            tokio::time::Duration::from_secs(120),
            tokio::spawn(async move {
                // Await connection and receive version
                let (mut peer_stream, _) = peer.accept().await.unwrap();
                let version = Message::read_from_stream(&mut peer_stream).await.unwrap();
                assert_matches!(version, Message::Version(..));

                // send bad version
                let _ = peer_stream.write_all(&payload).await;

                autorespond_and_expect_disconnect(&mut peer_stream).await;
            }),
        ));
    }

    let mut node: Node = Default::default();
    node.initial_action(Action::None)
        .initial_peers(peer_addresses)
        .start()
        .await;

    // join the peer processes
    for handle in peer_handles {
        handle.await.unwrap().unwrap();
    }

    node.stop().await;
}

#[tokio::test]
async fn incorrect_checksum_inplace_of_verack_when_node_initiates_handshake() {
    // ZG-RESISTANCE-004 (part 5)
    //
    // zebra: disconnects immediately.
    // zcashd: Messages get ignored (some get logged as bad checksum),
    //         node sends GetAddr, Ping and GetHeaders.

    let mut rng = seeded_rng();

    let test_messages = default_fuzz_messages();

    let mut payloads = encode_messages_and_corrupt_checksum(&mut rng, ITERATIONS, &test_messages);

    // create tcp listeners for peer set (port is only assigned on tcp bind)
    let mut listeners = Vec::with_capacity(ITERATIONS);
    for _ in 0..ITERATIONS {
        listeners.push(TcpListener::bind(new_local_addr()).await.unwrap());
    }

    // get list of peer addresses to pass to node
    let peer_addresses = listeners
        .iter()
        .map(|listener| listener.local_addr().unwrap())
        .collect::<Vec<_>>();

    // start peer processes
    let mut peer_handles = Vec::with_capacity(listeners.len());
    for peer in listeners {
        let payload = payloads.pop().unwrap();

        peer_handles.push(tokio::time::timeout(
            tokio::time::Duration::from_secs(120),
            tokio::spawn(async move {
                // Await connection and receive version
                let (mut peer_stream, _) = peer.accept().await.unwrap();
                let version = Message::read_from_stream(&mut peer_stream).await.unwrap();
                assert_matches!(version, Message::Version(..));

                // send version, receive verack
                Message::Version(Version::new(
                    peer_stream.peer_addr().unwrap(),
                    peer_stream.local_addr().unwrap(),
                ))
                .write_to_stream(&mut peer_stream)
                .await
                .unwrap();
                let verack = Message::read_from_stream(&mut peer_stream).await.unwrap();
                assert_matches!(verack, Message::Verack);

                // send bad verack
                let _ = peer_stream.write_all(&payload).await;

                autorespond_and_expect_disconnect(&mut peer_stream).await;
            }),
        ));
    }

    let mut node: Node = Default::default();
    node.initial_action(Action::None)
        .initial_peers(peer_addresses)
        .start()
        .await;

    // join the peer processes
    for handle in peer_handles {
        handle.await.unwrap().unwrap();
    }

    node.stop().await;
}

#[tokio::test]
async fn incorrect_checksum_post_handshake() {
    // ZG-RESISTANCE-005 (part 5)
    //
    // zebra: disconnects.
    // zcashd: logs indicate message was ignored, doesn't disconnect.

    let mut rng = seeded_rng();
    let mut node: Node = Default::default();
    node.initial_action(Action::WaitForConnection).start().await;

    let test_messages = default_fuzz_messages();

    for _ in 0..ITERATIONS {
        let message = test_messages.choose(&mut rng).unwrap();
        let mut message_buffer = vec![];
        let mut header = message.encode(&mut message_buffer).unwrap();

        // Set the checksum to a random value which isn't the current value.
        header.checksum = random_non_valid_u32(&mut rng, header.checksum);

        let mut peer_stream = initiate_handshake(node.addr()).await.unwrap();

        // Write messages with wrong checksum.
        let _ = header.write_to_stream(&mut peer_stream).await;
        let _ = peer_stream.write_all(&message_buffer).await;

        autorespond_and_expect_disconnect(&mut peer_stream).await;
    }

    node.stop().await;
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
