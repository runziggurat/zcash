pub mod bad_checksum;
pub mod incorrect_length;

use crate::{
    protocol::payload::codec::Codec,
    setup::node::{Action, Node},
    tests::resistance::{
        default_fuzz_messages, seeded_rng, Message, DISCONNECT_TIMEOUT, ITERATIONS,
    },
    tools::synthetic_node::SyntheticNode,
};

use assert_matches::assert_matches;
use rand::prelude::{Rng, SliceRandom};
use rand_chacha::ChaCha8Rng;

const CORRUPTION_PROBABILITY: f64 = 0.5;

#[tokio::test]
async fn corrupted_messages_pre_handshake() {
    // ZG-RESISTANCE-001 (part 4)
    //
    // zebra: responds with a version before disconnecting (however, quite slow running).
    // zcashd: just ignores the message and doesn't disconnect.

    let test_messages = default_fuzz_messages();

    let mut rng = seeded_rng();
    let payloads = slightly_corrupted_messages(&mut rng, ITERATIONS, &test_messages);

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
async fn corrupted_messages_during_handshake_responder_side() {
    // ZG-RESISTANCE-002 (part 4)
    //
    // zebra: responds with verack before disconnecting (however, quite slow running).
    // zcashd: Some variants result in a terminated connect, some get ignored.

    let test_messages = default_fuzz_messages();

    let mut rng = seeded_rng();
    let payloads = slightly_corrupted_messages(&mut rng, ITERATIONS, &test_messages);

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
async fn corrupted_messages_inplace_of_version_when_node_initiates_handshake() {
    // ZG-RESISTANCE-003 (part 4)
    //
    // zebra: disconnects immediately.
    // zcashd: Some messages get ignored and timeout.
    //
    // Note: zcashd is two orders of magnitude slower (~52 vs ~0.5 seconds)

    let test_messages = default_fuzz_messages();

    let mut rng = seeded_rng();
    let mut payloads = slightly_corrupted_messages(&mut rng, ITERATIONS, &test_messages);

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
async fn corrupted_messages_inplace_of_verack_when_node_initiates_handshake() {
    // ZG-RESISTANCE-004 (part 4)
    //
    // zebra: disconnects immediately.
    // zcashd: Some messages get ignored and timeout. Otherwise sends GetAddr, Ping and GetHeaders
    //         before disconnecting.
    //
    // Note: zcashd is two orders of magnitude slower (~52 vs ~0.5 seconds)

    let test_messages = default_fuzz_messages();

    let mut rng = seeded_rng();
    let mut payloads = slightly_corrupted_messages(&mut rng, ITERATIONS, &test_messages);

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
async fn corrupted_messages_post_handshake() {
    // ZG-RESISTANCE-005 (part 4)
    //
    // zebra: sends getdata and ignores message.
    // zcashd: disconnects for some messages, hangs for others.

    let test_messages = default_fuzz_messages();

    let mut rng = seeded_rng();
    let payloads = slightly_corrupted_messages(&mut rng, ITERATIONS, &test_messages);

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
