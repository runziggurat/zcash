pub mod bad_checksum;
pub mod bad_length;
pub mod random_payload;

use crate::{
    protocol::message::Message,
    setup::node::{Action, Node},
    tests::resistance::{DISCONNECT_TIMEOUT, ITERATIONS},
    tools::{
        fuzzing::{default_fuzz_messages, seeded_rng, encode_slightly_corrupted_messages},
        synthetic_node::SyntheticNode,
    },
};

use assert_matches::assert_matches;

#[tokio::test]
async fn instead_of_version_when_node_receives_connection() {
    // ZG-RESISTANCE-001 (part 4)
    //
    // zebra: responds with a version before disconnecting (however, quite slow running).
    // zcashd: just ignores the message and doesn't disconnect.

    let test_messages = default_fuzz_messages();

    let mut rng = seeded_rng();
    let payloads = encode_slightly_corrupted_messages(&mut rng, ITERATIONS, &test_messages);

    let mut node = Node::new().unwrap();
    node.initial_action(Action::WaitForConnection)
        .start()
        .await
        .unwrap();

    let synth_builder = SyntheticNode::builder().with_all_auto_reply();

    for payload in payloads {
        let mut synth_node = synth_builder.build().await.unwrap();
        synth_node.connect(node.addr()).await.unwrap();

        synth_node
            .send_direct_bytes(node.addr(), payload)
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
    // ZG-RESISTANCE-002 (part 4)
    //
    // zebra: responds with verack before disconnecting (however, quite slow running).
    // zcashd: Some variants result in a terminated connect, some get ignored.

    let test_messages = default_fuzz_messages();

    let mut rng = seeded_rng();
    let payloads = encode_slightly_corrupted_messages(&mut rng, ITERATIONS, &test_messages);

    let mut node = Node::new().unwrap();
    node.initial_action(Action::WaitForConnection)
        .start()
        .await
        .unwrap();

    let synth_builder = SyntheticNode::builder()
        .with_version_exchange_handshake()
        .with_all_auto_reply();

    for payload in payloads {
        let mut synth_node = synth_builder.build().await.unwrap();
        synth_node.connect(node.addr()).await.unwrap();

        // Write the corrupted message in place of Verack.
        synth_node
            .send_direct_bytes(node.addr(), payload)
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
    // ZG-RESISTANCE-003 (part 4)
    //
    // zebra: disconnects immediately.
    // zcashd: Some messages get ignored and timeout.
    //
    // Note: zcashd is two orders of magnitude slower (~52 vs ~0.5 seconds)

    let test_messages = default_fuzz_messages();

    let mut rng = seeded_rng();
    let mut payloads = encode_slightly_corrupted_messages(&mut rng, ITERATIONS, &test_messages);

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
    // ZG-RESISTANCE-004 (part 4)
    //
    // zebra: disconnects immediately.
    // zcashd: Some messages get ignored and timeout. Otherwise sends GetAddr, Ping and GetHeaders
    //         before disconnecting.
    //
    // Note: zcashd is two orders of magnitude slower (~52 vs ~0.5 seconds)

    let test_messages = default_fuzz_messages();

    let mut rng = seeded_rng();
    let mut payloads = encode_slightly_corrupted_messages(&mut rng, ITERATIONS, &test_messages);

    // create peers (we need their ports to give to the node)
    let (synth_nodes, synth_addrs) = SyntheticNode::builder()
        .with_version_exchange_handshake()
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
                // Await connection
                let node_addr = synth_node.wait_for_connection().await;

                // Receive verack
                let (_, verack) = synth_node.recv_message().await;
                assert_matches!(verack, Message::Verack);

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
    // ZG-RESISTANCE-005 (part 4)
    //
    // zebra: sends getdata and ignores message.
    // zcashd: disconnects for some messages, hangs for others.

    let test_messages = default_fuzz_messages();

    let mut rng = seeded_rng();
    let payloads = encode_slightly_corrupted_messages(&mut rng, ITERATIONS, &test_messages);

    let mut node = Node::new().unwrap();
    node.initial_action(Action::WaitForConnection)
        .start()
        .await
        .unwrap();

    let synth_builder = SyntheticNode::builder()
        .with_all_auto_reply()
        .with_full_handshake();

    for payload in payloads {
        let mut synth_node = synth_builder.build().await.unwrap();
        synth_node.connect(node.addr()).await.unwrap();

        // Write the corrupted message in place of Verack.
        synth_node
            .send_direct_bytes(node.addr(), payload)
            .await
            .unwrap();

        assert!(synth_node
            .wait_for_disconnect(node.addr(), DISCONNECT_TIMEOUT)
            .await
            .is_ok());
    }

    node.stop().await.unwrap();
}
