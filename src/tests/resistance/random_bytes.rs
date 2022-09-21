use assert_matches::assert_matches;

use crate::{
    protocol::message::Message,
    setup::node::{Action, Node},
    tests::resistance::{DISCONNECT_TIMEOUT, ITERATIONS},
    tools::{
        fuzzing::{random_bytes, seeded_rng},
        synthetic_node::SyntheticNode,
    },
};

#[tokio::test]
async fn r001_t2_instead_of_version_when_node_receives_connection() {
    // ZG-RESISTANCE-001 (part 2)
    //
    // zebra: sends a version before disconnecting.
    // zcashd: ignores the bytes and disconnects.

    let mut rng = seeded_rng();
    let payloads = random_bytes(&mut rng, ITERATIONS);

    let mut node = Node::new().unwrap();
    node.initial_action(Action::WaitForConnection)
        .start()
        .await
        .unwrap();

    for payload in payloads {
        let mut synth_node = SyntheticNode::builder()
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
async fn r002_t2_instead_of_verack_when_node_receives_connection() {
    // ZG-RESISTANCE-002 (part 2)
    //
    // zebra: responds with verack before disconnecting.
    // zcashd: responds with verack, pong and getheaders before disconnecting.

    let mut rng = seeded_rng();
    let payloads = random_bytes(&mut rng, ITERATIONS);

    let mut node = Node::new().unwrap();
    node.initial_action(Action::WaitForConnection)
        .start()
        .await
        .unwrap();

    for payload in payloads {
        let mut synth_node = SyntheticNode::builder()
            .with_all_auto_reply()
            .with_version_exchange_handshake()
            .build()
            .await
            .unwrap();
        synth_node.connect(node.addr()).await.unwrap();

        // Write random bytes in place of Verack.
        synth_node.send_direct_bytes(node.addr(), payload).unwrap();

        assert!(synth_node
            .wait_for_disconnect(node.addr(), DISCONNECT_TIMEOUT)
            .await
            .is_ok());
    }

    node.stop().unwrap();
}

#[tokio::test]
async fn r003_t2_instead_of_version_when_node_initiates_connection() {
    // ZG-RESISTANCE-003 (part 2)
    //
    // zebra: disconnects immediately.
    // zcashd: disconnects immediately.
    //
    // Note: zcashd is two orders of magnitude slower (~52 vs ~0.5 seconds)

    let mut rng = seeded_rng();
    let mut payloads = random_bytes(&mut rng, ITERATIONS);

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
async fn r004_t2_instead_of_verack_when_node_initiates_connection() {
    // ZG-RESISTANCE-004 (part 2)
    //
    // zebra: disconnects immediately.
    // zcashd: sometimes (~10%) sends GetAddr before disconnecting
    //
    // Note: zcashd is two orders of magnitude slower (~52 vs ~0.5 seconds)

    let mut rng = seeded_rng();
    let mut payloads = random_bytes(&mut rng, ITERATIONS);

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
async fn r005_t2_post_handshake() {
    // ZG-RESISTANCE-005 (part 2)
    //
    // zebra: disconnects.
    // zcashd: sends ping, getheaders and disconnects.

    let mut rng = seeded_rng();
    let payloads = random_bytes(&mut rng, ITERATIONS);

    let mut node = Node::new().unwrap();
    node.initial_action(Action::WaitForConnection)
        .start()
        .await
        .unwrap();

    for payload in payloads {
        let mut synth_node = SyntheticNode::builder()
            .with_all_auto_reply()
            .with_full_handshake()
            .build()
            .await
            .unwrap();
        synth_node.connect(node.addr()).await.unwrap();

        // Write random bytes in place of Verack.
        synth_node.send_direct_bytes(node.addr(), payload).unwrap();
        assert!(synth_node
            .wait_for_disconnect(node.addr(), DISCONNECT_TIMEOUT)
            .await
            .is_ok());
    }

    node.stop().unwrap();
}
