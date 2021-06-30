use crate::{
    protocol::{
        message::{constants::HEADER_LEN, Message},
        payload::codec::Codec,
    },
    setup::node::{Action, Node},
    tests::resistance::{DISCONNECT_TIMEOUT, ITERATIONS},
    tools::{
        fuzzing::{
            default_fuzz_messages, encode_messages_and_corrupt_checksum, random_non_valid_u32,
            seeded_rng,
        },
        synthetic_node::SyntheticNode,
    },
};

use assert_matches::assert_matches;
use rand::prelude::SliceRandom;

#[tokio::test]
async fn instead_of_version_when_node_receives_connection() {
    // ZG-RESISTANCE-001 (part 5)
    //
    // zebra: sends a version before disconnecting.
    // zcashd: ignores the messages but doesn't disconnect (logs show a `CHECKSUM ERROR`).

    let mut rng = seeded_rng();

    let mut node = Node::new().unwrap();
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

    node.stop().await.unwrap();
}

#[tokio::test]
async fn instead_of_verack_when_node_receives_connection() {
    // ZG-RESISTANCE-002 (part 5)
    //
    // zebra: sends a verack before disconnecting.
    // zcashd: logs indicate message was ignored, doesn't disconnect.

    let mut rng = seeded_rng();

    let mut node = Node::new().unwrap();
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

    node.stop().await.unwrap();
}

#[tokio::test]
async fn instead_of_version_when_node_initiates_connection() {
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

    let mut node = Node::new().unwrap();
    node.initial_action(Action::None)
        .initial_peers(synth_addrs)
        .start()
        .await
        .unwrap();

    // join the peer processes
    for handle in synth_handles {
        handle.await.unwrap().unwrap();
    }

    node.stop().await.unwrap();
}

#[tokio::test]
async fn instead_of_verack_when_node_initiates_connection() {
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

    let mut node = Node::new().unwrap();
    node.initial_action(Action::None)
        .initial_peers(synth_addrs)
        .start()
        .await
        .unwrap();

    // join the peer processes
    for handle in synth_handles {
        handle.await.unwrap().unwrap();
    }

    node.stop().await.unwrap();
}

#[tokio::test]
async fn post_handshake() {
    // ZG-RESISTANCE-005 (part 5)
    //
    // zebra: disconnects.
    // zcashd: logs indicate message was ignored, doesn't disconnect.

    let mut rng = seeded_rng();
    let mut node = Node::new().unwrap();
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

    node.stop().await.unwrap();
}
