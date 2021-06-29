use crate::{
    protocol::{
        message::{constants::HEADER_LEN, Message},
        payload::codec::Codec,
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
async fn incorrect_length_pre_handshake() {
    // ZG-RESISTANCE-001 (part 6)
    //
    // zebra: sends version before disconnecting.
    // zcashd: disconnects.

    let mut rng = seeded_rng();

    let mut node = Node::new().unwrap();
    node.initial_action(Action::WaitForConnection)
        .start()
        .await
        .unwrap();

    let test_messages = default_fuzz_messages();

    for _ in 0..ITERATIONS {
        let mut synth_node = SyntheticNode::builder()
            .with_all_auto_reply()
            .build()
            .await
            .unwrap();
        synth_node.connect(node.addr()).await.unwrap();

        let message = test_messages.choose(&mut rng).unwrap();
        let payload = encode_with_corrupt_body_length(&mut rng, message);

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
async fn incorrect_length_during_handshake_responder_side() {
    // ZG-RESISTANCE-002 (part 6)
    //
    // zebra: disconnects.
    // zcashd: disconnects (after sending verack).

    let mut rng = seeded_rng();

    let mut node = Node::new().unwrap();
    node.initial_action(Action::WaitForConnection)
        .start()
        .await
        .unwrap();

    let test_messages = default_fuzz_messages();

    for _ in 0..ITERATIONS {
        let mut synth_node = SyntheticNode::builder()
            .with_all_auto_reply()
            .with_version_exchange_handshake()
            .build()
            .await
            .unwrap();
        synth_node.connect(node.addr()).await.unwrap();

        let message = test_messages.choose(&mut rng).unwrap();
        let payload = encode_with_corrupt_body_length(&mut rng, message);

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
async fn incorrect_body_length_inplace_of_version_when_node_initiates_handshake() {
    // ZG-RESISTANCE-003 (part 6)
    //
    // zebra: disconnects
    // zcashd: disconnects, very slow running

    let mut rng = seeded_rng();

    let test_messages = default_fuzz_messages();

    let mut payloads =
        encode_messages_and_corrupt_body_length_field(&mut rng, ITERATIONS, &test_messages);

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
                synth_node.wait_for_connection().await;
                let (node_addr, version) = synth_node.recv_message().await;
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
async fn incorrect_body_length_inplace_of_verack_when_node_initiates_handshake() {
    // ZG-RESISTANCE-004 (part 6)
    //
    // zebra: disconnects, logs:
    //  - an initial peer connection failed e=Serialization(Parse("body length exceeded maximum size"))
    //  - an initial peer connection failed e=Serialization(Parse("supplied magic did not meet expectations")) [?]
    //
    // zcashd: sends GetAddr, Ping, GetHeaders then disconnects

    let mut rng = seeded_rng();

    let test_messages = default_fuzz_messages();

    let mut payloads =
        encode_messages_and_corrupt_body_length_field(&mut rng, ITERATIONS, &test_messages);

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
                // Await connection and receive verack.
                // Version exchange already completed by handshake.
                synth_node.wait_for_connection().await;
                let (node_addr, verack) = synth_node.recv_message().await;
                assert_matches!(verack, Message::Verack);

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
async fn incorrect_length_post_handshake() {
    // ZG-RESISTANCE-005 (part 6)
    //
    // zebra: disconnects.
    // zcashd: disconnects (sometimes sends ping and getheaders)

    let mut rng = seeded_rng();
    let mut node = Node::new().unwrap();
    node.initial_action(Action::WaitForConnection)
        .start()
        .await
        .unwrap();

    let test_messages = default_fuzz_messages();

    for _ in 0..ITERATIONS {
        let mut synth_node = SyntheticNode::builder()
            .with_all_auto_reply()
            .with_full_handshake()
            .build()
            .await
            .unwrap();
        synth_node.connect(node.addr()).await.unwrap();

        let message = test_messages.choose(&mut rng).unwrap();
        let payload = encode_with_corrupt_body_length(&mut rng, message);

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

fn encode_with_corrupt_body_length(rng: &mut ChaCha8Rng, message: &Message) -> Vec<u8> {
    let mut body_buffer = Vec::new();
    let mut header = message.encode(&mut body_buffer).unwrap();

    let mut buffer = Vec::with_capacity(body_buffer.len() + HEADER_LEN);
    header.body_length = random_non_valid_u32(rng, header.body_length);
    header.encode(&mut buffer).unwrap();
    buffer.append(&mut body_buffer);

    buffer
}

/// Picks `n` random messages from `message_pool`, encodes them and corrupts the body-length field
pub fn encode_messages_and_corrupt_body_length_field(
    rng: &mut ChaCha8Rng,
    n: usize,
    message_pool: &[Message],
) -> Vec<Vec<u8>> {
    (0..n)
        .map(|_| {
            let message = message_pool.choose(rng).unwrap();

            encode_with_corrupt_body_length(rng, &message)
        })
        .collect()
}
