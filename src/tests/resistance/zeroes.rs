//! Contains fuzz tests where messages are replaced with random length payloads of 0x00.
use std::cmp;

use crate::{
    protocol::message::Message,
    setup::node::{Action, Node},
    tests::resistance::{DISCONNECT_TIMEOUT, ITERATIONS},
    tools::{
        fuzzing::{seeded_rng, zeroes},
        synthetic_node::SyntheticNode,
    },
};

use assert_matches::assert_matches;

#[tokio::test]
async fn instead_of_version_when_node_receives_connection() {
    // ZG-RESISTANCE-001 (part 1)
    //
    // zebra: sends a version before disconnecting.
    // zcashd: disconnects immediately (log: `INFO main: PROCESSMESSAGE: INVALID MESSAGESTART peer=1`).

    let mut rng = seeded_rng();
    let payloads = zeroes(&mut rng, ITERATIONS);

    let mut node = Node::new().unwrap();
    node.initial_action(Action::WaitForConnection)
        .start()
        .await
        .unwrap();

    for payload in payloads {
        let mut synth_node = SyntheticNode::builder()
            .with_max_write_buffer_size(cmp::max(payload.len(), 65536))
            .with_all_auto_reply()
            .build()
            .await
            .unwrap();
        synth_node.connect(node.addr()).await.unwrap();

        synth_node.send_direct_bytes(node.addr(), payload).unwrap();

        assert!(synth_node
            .wait_for_disconnect(node.addr(), DISCONNECT_TIMEOUT)
            .await
            .is_ok());
    }

    node.stop().unwrap();
}

#[tokio::test]
async fn instead_of_verack_when_node_receives_connection() {
    // ZG-RESISTANCE-002 (part 1)
    //
    // zebra: responds with verack before disconnecting.
    // zcashd: disconnects immediately.

    let mut rng = seeded_rng();
    let payloads = zeroes(&mut rng, ITERATIONS);

    let mut node = Node::new().unwrap();
    node.initial_action(Action::WaitForConnection)
        .start()
        .await
        .unwrap();

    for payload in payloads {
        let mut synth_node = SyntheticNode::builder()
            .with_all_auto_reply()
            .with_version_exchange_handshake()
            .with_max_write_buffer_size(cmp::max(payload.len(), 65536))
            .build()
            .await
            .unwrap();
        synth_node.connect(node.addr()).await.unwrap();

        // Write zeroes in place of Verack.
        synth_node.send_direct_bytes(node.addr(), payload).unwrap();

        assert!(synth_node
            .wait_for_disconnect(node.addr(), DISCONNECT_TIMEOUT)
            .await
            .is_ok());
    }

    node.stop().unwrap();
}

#[tokio::test]
async fn instead_of_version_when_node_initiates_connection() {
    // ZG-RESISTANCE-003 (part 1)
    //
    // zebra: disconnects immediately.
    // zcashd: disconnects immediately.
    //
    // Note: zcashd is two orders of magnitude slower (~52 vs ~0.5 seconds)

    let mut rng = seeded_rng();
    let mut payloads = zeroes(&mut rng, ITERATIONS);

    let max_payload = payloads.iter().map(|p| p.len()).max().unwrap().max(65536);

    // create peers (we need their ports to give to the node)
    let (synth_nodes, synth_addrs) = SyntheticNode::builder()
        .with_all_auto_reply()
        .with_max_write_buffer_size(max_payload)
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
                synth_node.send_direct_bytes(node_addr, payload).unwrap();

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

    node.stop().unwrap();
}

#[tokio::test]
async fn instead_of_verack_when_node_initiates_connection() {
    // ZG-RESISTANCE-004 (part 1)
    //
    // zebra: disconnects immediately.
    // zcashd: sends GetAddr, Ping, GetHeaders before disconnecting
    //
    // Note: zcashd is two orders of magnitude slower (~52 vs ~0.5 seconds)

    let mut rng = seeded_rng();
    let mut payloads = zeroes(&mut rng, ITERATIONS);

    let max_payload = payloads.iter().map(|p| p.len()).max().unwrap().max(65536);

    // create peers (we need their ports to give to the node)
    let (synth_nodes, synth_addrs) = SyntheticNode::builder()
        .with_all_auto_reply()
        .with_version_exchange_handshake()
        .with_max_write_buffer_size(max_payload)
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
                let node_addr = synth_node.wait_for_connection().await;

                let (_, verack) = synth_node.recv_message().await;
                assert_matches!(verack, Message::Verack);

                // send bad verack
                synth_node.send_direct_bytes(node_addr, payload).unwrap();

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

    node.stop().unwrap();
}

#[tokio::test]
async fn post_handshake() {
    // ZG-RESISTANCE-005 (part 1)
    //
    // zebra: disconnects.
    // zcashd: responds with ping and getheaders before disconnecting.

    let mut rng = seeded_rng();
    let payloads = zeroes(&mut rng, ITERATIONS);

    let mut node = Node::new().unwrap();
    node.initial_action(Action::WaitForConnection)
        .start()
        .await
        .unwrap();

    for payload in payloads {
        let mut synth_node = SyntheticNode::builder()
            .with_all_auto_reply()
            .with_full_handshake()
            .with_max_write_buffer_size(cmp::max(payload.len(), 65536))
            .build()
            .await
            .unwrap();
        synth_node.connect(node.addr()).await.unwrap();

        synth_node.send_direct_bytes(node.addr(), payload).unwrap();

        assert!(synth_node
            .wait_for_disconnect(node.addr(), DISCONNECT_TIMEOUT)
            .await
            .is_ok());
    }

    node.stop().unwrap();
}
