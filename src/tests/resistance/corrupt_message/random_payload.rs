use std::cmp;

use crate::{
    protocol::message::Message,
    setup::node::{Action, Node},
    tests::resistance::{DISCONNECT_TIMEOUT, ITERATIONS},
    tools::{
        fuzzing::{metadata_compliant_random_bytes, seeded_rng, COMMANDS_WITH_PAYLOADS},
        synthetic_node::SyntheticNode,
    },
};

use assert_matches::assert_matches;

#[tokio::test]
async fn instead_of_version_when_node_receives_connection() {
    // ZG-RESISTANCE-001 (part 3)
    //
    // zebra: breaks with a version command in header, otherwise sends verack before closing the
    // connection.
    // zcashd: just ignores the message and doesn't disconnect.

    // Payloadless messages are omitted.
    let mut rng = seeded_rng();
    let payloads = metadata_compliant_random_bytes(&mut rng, ITERATIONS, &COMMANDS_WITH_PAYLOADS);

    let mut node = Node::new().unwrap();
    node.initial_action(Action::WaitForConnection)
        .start()
        .await
        .unwrap();

    for payload in payloads {
        let mut synth_node = SyntheticNode::builder()
            .with_all_auto_reply()
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

#[tokio::test]
async fn instead_of_verack_when_node_receives_connection() {
    // ZG-RESISTANCE-002 (part 3)
    //
    // zebra: breaks with a version command in header, otherwise sends verack before closing the
    // connection.
    // zcashd: responds with reject, ccode malformed and doesn't disconnect.

    // Payloadless messages are omitted.
    let mut rng = seeded_rng();
    let payloads = metadata_compliant_random_bytes(&mut rng, ITERATIONS, &COMMANDS_WITH_PAYLOADS);

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
    // ZG-RESISTANCE-003 (part 3)
    //
    // zebra: breaks with a version command in header, otherwise sends verack before closing the
    //        connection.
    // zcashd: just ignores the message and doesn't disconnect.
    //
    // Note: zcashd is two orders of magnitude slower (~52 vs ~0.5 seconds)

    // Payloadless messages are omitted.
    let mut rng = seeded_rng();
    let mut payloads =
        metadata_compliant_random_bytes(&mut rng, ITERATIONS, &COMMANDS_WITH_PAYLOADS);
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
    // ZG-RESISTANCE-004 (part 3)
    //
    // zebra: breaks with a version command in header, otherwise sends verack before closing the
    //        connection.
    // zcashd: Sends GetAddr, Ping, GetHeaders
    //         Sometimes responds to malformed Ping's
    //         Never disconnects
    //
    // Caution: zcashd takes extremely long in this test

    // Payloadless messages are omitted.
    let mut rng = seeded_rng();
    let mut payloads =
        metadata_compliant_random_bytes(&mut rng, ITERATIONS, &COMMANDS_WITH_PAYLOADS);
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
                synth_node.wait_for_connection().await;
                let (node_addr, verack) = synth_node.recv_message().await;
                assert_matches!(verack, Message::Verack);

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
async fn post_handshake() {
    // ZG-RESISTANCE-005 (part 3)
    //
    // zebra: breaks with a version command in header, spams getdata, doesn't disconnect.
    // zcashd: does a combination of ignoring messages, returning cc malformed or accepting messages (`addr`)
    // for instance.

    // Payloadless messages are omitted.
    let mut rng = seeded_rng();
    let payloads = metadata_compliant_random_bytes(&mut rng, ITERATIONS, &COMMANDS_WITH_PAYLOADS);

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
