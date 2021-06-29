use crate::{
    protocol::{
        message::{constants::HEADER_LEN, Message},
        payload::{codec::Codec, Version},
    },
    setup::node::{Action, Node},
    tests::resistance::{
        default_fuzz_messages, random_non_valid_u32, seeded_rng, DISCONNECT_TIMEOUT, ITERATIONS,
    },
    tools::synthetic_node::SyntheticNode,
};

use assert_matches::assert_matches;
use rand::prelude::SliceRandom;
use rand_chacha::ChaCha8Rng;

#[tokio::test]
async fn version_with_incorrect_checksum_pre_handshake() {
    // ZG-RESISTANCE-001 (part 5)
    //
    // zebra: sends version before disconnecting.
    // zcashd: log suggests messages was ignored, doesn't disconnect.

    let mut rng = seeded_rng();

    let mut node: Node = Default::default();
    node.initial_action(Action::WaitForConnection)
        .start()
        .await
        .unwrap();

    for _ in 0..ITERATIONS {
        let mut synth_node = SyntheticNode::builder()
            .with_all_auto_reply()
            .build()
            .await
            .unwrap();
        synth_node.connect(node.addr()).await.unwrap();

        let version = Message::Version(Version::new(node.addr(), synth_node.listening_addr()));
        let mut body_buffer = Vec::new();
        let mut header = version.encode(&mut body_buffer).unwrap();

        // Set the checksum to a random value which isn't the current value.
        header.checksum = random_non_valid_u32(&mut rng, header.checksum);

        let mut buffer = Vec::new();
        header.encode(&mut buffer).unwrap();
        buffer.append(&mut body_buffer);

        synth_node
            .send_direct_bytes(node.addr(), buffer)
            .await
            .unwrap();

        assert!(synth_node
            .wait_for_disconnect(node.addr(), DISCONNECT_TIMEOUT)
            .await
            .is_ok());
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
    node.initial_action(Action::WaitForConnection)
        .start()
        .await
        .unwrap();

    let test_messages = default_fuzz_messages();

    for _ in 0..ITERATIONS {
        let message = test_messages.choose(&mut rng).unwrap();
        let mut body_buffer = Vec::new();
        let mut header = message.encode(&mut body_buffer).unwrap();

        // Set the checksum to a random value which isn't the current value.
        header.checksum = random_non_valid_u32(&mut rng, header.checksum);
        let mut buffer = Vec::new();
        header.encode(&mut buffer).unwrap();
        buffer.append(&mut body_buffer);

        let mut synth_node = SyntheticNode::builder()
            .with_all_auto_reply()
            .build()
            .await
            .unwrap();
        synth_node.connect(node.addr()).await.unwrap();

        synth_node
            .send_direct_bytes(node.addr(), buffer)
            .await
            .unwrap();

        assert!(synth_node
            .wait_for_disconnect(node.addr(), DISCONNECT_TIMEOUT)
            .await
            .is_ok());
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
    node.initial_action(Action::WaitForConnection)
        .start()
        .await
        .unwrap();

    for _ in 0..ITERATIONS {
        let mut synth_node = SyntheticNode::builder()
            .with_all_auto_reply()
            .with_version_exchange_handshake()
            .build()
            .await
            .unwrap();
        synth_node.connect(node.addr()).await.unwrap();

        let version = Message::Version(Version::new(node.addr(), synth_node.listening_addr()));
        let mut body_buffer = Vec::new();
        let mut header = version.encode(&mut body_buffer).unwrap();

        // Set the checksum to a random value which isn't the current value.
        header.checksum = random_non_valid_u32(&mut rng, header.checksum);

        let mut buffer = Vec::with_capacity(HEADER_LEN + body_buffer.len());
        header.encode(&mut buffer).unwrap();
        buffer.append(&mut body_buffer);

        synth_node
            .send_direct_bytes(node.addr(), buffer)
            .await
            .unwrap();

        assert!(synth_node
            .wait_for_disconnect(node.addr(), DISCONNECT_TIMEOUT)
            .await
            .is_ok());
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
    node.initial_action(Action::WaitForConnection)
        .start()
        .await
        .unwrap();

    for _ in 0..ITERATIONS {
        let mut synth_node = SyntheticNode::builder()
            .with_full_handshake()
            .with_all_auto_reply()
            .build()
            .await
            .unwrap();
        synth_node.connect(node.addr()).await.unwrap();

        let version = Message::Version(Version::new(node.addr(), synth_node.listening_addr()));
        let mut body_buffer = Vec::new();
        let mut header = version.encode(&mut body_buffer).unwrap();

        // Set the checksum to a random value which isn't the current value.
        header.checksum = random_non_valid_u32(&mut rng, header.checksum);

        let mut buffer = Vec::with_capacity(HEADER_LEN + body_buffer.len());
        header.encode(&mut buffer).unwrap();
        buffer.append(&mut body_buffer);

        synth_node
            .send_direct_bytes(node.addr(), buffer)
            .await
            .unwrap();

        assert!(synth_node
            .wait_for_disconnect(node.addr(), DISCONNECT_TIMEOUT)
            .await
            .is_ok());
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
    node.initial_action(Action::WaitForConnection)
        .start()
        .await
        .unwrap();

    let test_messages = default_fuzz_messages();

    for _ in 0..ITERATIONS {
        let message = test_messages.choose(&mut rng).unwrap();
        let mut body_buffer = Vec::new();
        let mut header = message.encode(&mut body_buffer).unwrap();

        // Set the checksum to a random value which isn't the current value.
        header.checksum = random_non_valid_u32(&mut rng, header.checksum);

        let mut buffer = Vec::new();
        header.encode(&mut buffer).unwrap();
        buffer.append(&mut body_buffer);

        let mut synth_node = SyntheticNode::builder()
            .with_all_auto_reply()
            .with_version_exchange_handshake()
            .build()
            .await
            .unwrap();
        synth_node.connect(node.addr()).await.unwrap();

        synth_node
            .send_direct_bytes(node.addr(), buffer)
            .await
            .unwrap();

        assert!(synth_node
            .wait_for_disconnect(node.addr(), DISCONNECT_TIMEOUT)
            .await
            .is_ok());
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

    // create peers (we need their ports to give to the node)
    let (synth_nodes, synth_addrs) = SyntheticNode::builder()
        .with_all_auto_reply()
        .build_n(ITERATIONS)
        .await
        .unwrap();

    // start peer processes
    let mut synth_handles = Vec::with_capacity(synth_nodes.len());
    for mut synth_node in synth_nodes {
        let payload = payloads.pop().unwrap();

        synth_handles.push(tokio::time::timeout(
            tokio::time::Duration::from_secs(120),
            tokio::spawn(async move {
                // Await connection and receive version
                let node_addr = synth_node.wait_for_connection().await;
                let (_, version) = synth_node.recv_message().await;
                assert_matches!(version, Message::Version(..));

                // send bad version
                synth_node
                    .send_direct_bytes(node_addr, payload)
                    .await
                    .unwrap();

                assert!(synth_node
                    .wait_for_disconnect(node_addr, DISCONNECT_TIMEOUT)
                    .await
                    .is_ok());
            }),
        ));
    }

    let mut node: Node = Default::default();
    node.initial_action(Action::None)
        .initial_peers(synth_addrs)
        .start()
        .await
        .unwrap();

    // join the peer processes
    for handle in synth_handles {
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

    // create peers (we need their ports to give to the node)
    let (synth_nodes, synth_addrs) = SyntheticNode::builder()
        .with_all_auto_reply()
        .with_version_exchange_handshake()
        .build_n(ITERATIONS)
        .await
        .unwrap();

    // start peer processes
    let mut synth_handles = Vec::with_capacity(synth_nodes.len());
    for mut synth_node in synth_nodes {
        let payload = payloads.pop().unwrap();

        synth_handles.push(tokio::time::timeout(
            tokio::time::Duration::from_secs(120),
            tokio::spawn(async move {
                // Await connection and receive verack,
                // version's already exchanged as part of handshake
                let node_addr = synth_node.wait_for_connection().await;
                let (_, verack) = synth_node.recv_message().await;
                assert_eq!(verack, Message::Verack);

                // send bad verack
                synth_node
                    .send_direct_bytes(node_addr, payload)
                    .await
                    .unwrap();

                assert!(synth_node
                    .wait_for_disconnect(node_addr, DISCONNECT_TIMEOUT)
                    .await
                    .is_ok());
            }),
        ));
    }

    let mut node: Node = Default::default();
    node.initial_action(Action::None)
        .initial_peers(synth_addrs)
        .start()
        .await
        .unwrap();

    // join the peer processes
    for handle in synth_handles {
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
    node.initial_action(Action::WaitForConnection)
        .start()
        .await
        .unwrap();

    let test_messages = default_fuzz_messages();

    for _ in 0..ITERATIONS {
        let message = test_messages.choose(&mut rng).unwrap();
        let mut body_buffer = Vec::new();
        let mut header = message.encode(&mut body_buffer).unwrap();

        // Set the checksum to a random value which isn't the current value.
        header.checksum = random_non_valid_u32(&mut rng, header.checksum);

        let mut buffer = Vec::with_capacity(HEADER_LEN + body_buffer.len());
        header.encode(&mut buffer).unwrap();
        buffer.append(&mut body_buffer);

        let mut synth_node = SyntheticNode::builder()
            .with_full_handshake()
            .with_all_auto_reply()
            .build()
            .await
            .unwrap();
        synth_node.connect(node.addr()).await.unwrap();

        // Write messages with wrong checksum.
        synth_node
            .send_direct_bytes(node.addr(), buffer)
            .await
            .unwrap();

        assert!(synth_node
            .wait_for_disconnect(node.addr(), DISCONNECT_TIMEOUT)
            .await
            .is_ok());
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
