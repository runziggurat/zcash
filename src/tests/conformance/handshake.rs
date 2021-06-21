use std::time::Duration;

use crate::{
    helpers::{
        synthetic_peers::{SyntheticNode, SyntheticNodeConfig},
        TIMEOUT,
    },
    protocol::{
        message::{filter::MessageFilter, Message},
        payload::{
            block::{Block, LocatorHashes},
            reject::CCode,
            Addr, Hash, Inv, Nonce, Version,
        },
    },
    setup::node::{Action, Node},
    wait_until,
};

use assert_matches::assert_matches;

#[tokio::test]
async fn handshake_responder_side() {
    // ZG-CONFORMANCE-001

    // Spin up a node instance.
    let mut node: Node = Default::default();
    node.initial_action(Action::WaitForConnection).start().await;

    // Create a synthetic node and enable handshaking.
    let synthetic_node = SyntheticNode::new(SyntheticNodeConfig {
        enable_handshaking: true,
        ..Default::default()
    })
    .await
    .unwrap();

    // Connect to the node and initiate the handshake.
    synthetic_node.connect(node.addr()).await.unwrap();

    // This is only set post-handshake (if enabled).
    assert!(synthetic_node.is_connected(node.addr()));

    // Gracefully shut down the nodes.
    synthetic_node.shut_down();
    node.stop().await;
}

#[tokio::test]
async fn handshake_initiator_side() {
    // ZG-CONFORMANCE-002

    // Create a synthetic node and enable handshaking.
    let synthetic_node = SyntheticNode::new(SyntheticNodeConfig {
        enable_handshaking: true,
        ..Default::default()
    })
    .await
    .unwrap();

    // Spin up a node and set the synthetic node as an initial peer.
    let mut node: Node = Default::default();
    node.initial_peers(vec![synthetic_node.listening_addr()])
        .start()
        .await;

    // Check the connection has been established (this is only set post-handshake). We can't check
    // for the addr as nodes use ephemeral addresses when initiating connections.
    wait_until!(Duration::from_secs(5), synthetic_node.num_connected() == 1);

    // Gracefully shut down the nodes.
    synthetic_node.shut_down();
    node.stop().await;
}

#[tokio::test]
async fn ignore_non_version_before_handshake() {
    // ZG-CONFORMANCE-003
    //
    // The node should ignore non-Version messages before the handshake has been performed.
    //
    // zebra: eagerly sends version but doesn't respnd to verack and disconnects.
    // zcashd: ignores the message and completes the handshake.

    let genesis_block = Block::testnet_genesis();
    let block_hash = genesis_block.double_sha256().unwrap();
    let block_inv = Inv::new(vec![genesis_block.inv_hash()]);
    let block_loc = LocatorHashes::new(vec![block_hash], Hash::zeroed());
    let test_messages = vec![
        Message::GetAddr,
        Message::MemPool,
        Message::Verack,
        Message::Ping(Nonce::default()),
        Message::Pong(Nonce::default()),
        Message::GetAddr,
        Message::Addr(Addr::empty()),
        Message::GetHeaders(block_loc.clone()),
        Message::GetBlocks(block_loc),
        Message::GetData(block_inv.clone()),
        Message::GetData(Inv::new(vec![genesis_block.txs[0].inv_hash()])),
        Message::Inv(block_inv.clone()),
        Message::NotFound(block_inv),
    ];

    // Spin up a node instance.
    let mut node: Node = Default::default();
    node.initial_action(Action::WaitForConnection).start().await;

    // Configuration to be used by all synthetic nodes, no handshaking, no message filters.
    let config: SyntheticNodeConfig = Default::default();

    for message in test_messages {
        let mut synthetic_node = SyntheticNode::new(config.clone()).await.unwrap();

        // Connect to the node, don't handshake.
        synthetic_node.connect(node.addr()).await.unwrap();

        // Send a non-version message.
        synthetic_node
            .send_direct_message(node.addr(), message)
            .await
            .unwrap();

        // Expect the node to ignore the previous message, verify by completing the handshake.
        // Send Version.
        synthetic_node
            .send_direct_message(
                node.addr(),
                Message::Version(Version::new(synthetic_node.listening_addr(), node.addr())),
            )
            .await
            .unwrap();

        // Read Version.
        let (_, version) = synthetic_node.recv_message_timeout(TIMEOUT).await.unwrap();
        assert_matches!(version, Message::Version(..));

        // Send Verack.
        synthetic_node
            .send_direct_message(node.addr(), Message::Verack)
            .await
            .unwrap();

        // Read Verack.
        let (_, verack) = synthetic_node.recv_message_timeout(TIMEOUT).await.unwrap();
        assert_matches!(verack, Message::Verack);

        // Gracefully shut down the synthetic node.
        synthetic_node.shut_down();
    }

    // Gracefully shut down the node.
    node.stop().await;
}

#[tokio::test]
async fn ignore_non_version_replies_to_version() {
    // ZG-CONFORMANCE-004
    //
    // The node should ignore non-Version messages in response to the initial Version it sent.
    //
    // Due to how we instrument the test node, we need to have the list of peers ready when we start the node.
    //
    // zebra: doesn't respond to verack and disconnects.
    // zcashd: ignores the message and completes the handshake.

    let genesis_block = Block::testnet_genesis();
    let block_hash = genesis_block.double_sha256().unwrap();
    let block_inv = Inv::new(vec![genesis_block.inv_hash()]);
    let block_loc = LocatorHashes::new(vec![block_hash], Hash::zeroed());
    let test_messages = vec![
        Message::GetAddr,
        Message::MemPool,
        Message::Verack,
        Message::Ping(Nonce::default()),
        Message::Pong(Nonce::default()),
        Message::GetAddr,
        Message::Addr(Addr::empty()),
        Message::GetHeaders(block_loc.clone()),
        Message::GetBlocks(block_loc),
        Message::GetData(block_inv.clone()),
        Message::GetData(Inv::new(vec![genesis_block.txs[0].inv_hash()])),
        Message::Inv(block_inv.clone()),
        Message::NotFound(block_inv),
    ];

    // Configuration to be used by all synthetic nodes, no handshaking, no message filters.
    let config: SyntheticNodeConfig = Default::default();
    // Instantiate a node instance without starting it (so we have access to its addr).
    let mut node: Node = Default::default();
    let node_addr = node.addr();

    // Store the listening addresses of the synthetic nodes so they can be set as initial peers of
    // the node.
    let mut listeners = Vec::with_capacity(test_messages.len());

    // Create a future for each message.
    let mut handles = Vec::with_capacity(test_messages.len());

    for message in test_messages {
        // Create a synthetic node and store its address.
        let mut synthetic_node = SyntheticNode::new(config.clone()).await.unwrap();
        listeners.push(synthetic_node.listening_addr());

        let handle = tokio::spawn(async move {
            // Receive Version.
            let (source, version) = synthetic_node.recv_message_timeout(TIMEOUT).await.unwrap();
            assert_matches!(version, Message::Version(..));

            // Send non-version.
            synthetic_node
                .send_direct_message(source, message)
                .await
                .unwrap();

            // Initiate the handshake by sending a Version.
            synthetic_node
                .send_direct_message(
                    source,
                    Message::Version(Version::new(synthetic_node.listening_addr(), node_addr)),
                )
                .await
                .unwrap();

            // Receiving a Verack indicates the non-version message was ignored.
            let (_, verack) = synthetic_node.recv_message_timeout(TIMEOUT).await.unwrap();
            assert_matches!(verack, Message::Verack);

            // Gracefully shut down the synthetic node.
            synthetic_node.shut_down();
        });

        handles.push(handle);
    }

    // Start the node instance with the initial peers.
    node.initial_peers(listeners).start().await;

    // Run each future to completion.
    for handle in handles {
        handle.await.unwrap();
    }

    // Gracefully shut down the node.
    node.stop().await;
}

#[tokio::test]
async fn ignore_non_verack_replies_to_verack() {
    // Conformance test 005.
    //
    // The node ignores non-Verack message as a response to initial Verack it sent.
    //
    // zebra: disconnects.
    // zcashd: responds to the unsolicited message.

    let genesis_block = Block::testnet_genesis();
    let block_hash = genesis_block.double_sha256().unwrap();
    let block_inv = Inv::new(vec![genesis_block.inv_hash()]);
    let block_loc = LocatorHashes::new(vec![block_hash], Hash::zeroed());
    let test_messages = vec![
        Message::GetAddr,
        Message::MemPool,
        Message::Ping(Nonce::default()),
        Message::Pong(Nonce::default()),
        Message::GetAddr,
        Message::Addr(Addr::empty()),
        Message::GetHeaders(block_loc.clone()),
        Message::GetBlocks(block_loc),
        Message::GetData(block_inv.clone()),
        Message::GetData(Inv::new(vec![genesis_block.txs[0].inv_hash()])),
        Message::Inv(block_inv.clone()),
        Message::NotFound(block_inv),
    ];

    // Configuration to be used by all synthetic nodes, no handshaking with filtering enabled so we
    // can assert on a ping pong exchange at the end of the test.
    let config = SyntheticNodeConfig {
        message_filter: MessageFilter::with_all_enabled(),
        ..Default::default()
    };

    // Instantiate a node instance without starting it (so we have access to its addr).
    let mut node: Node = Default::default();
    let node_addr = node.addr();

    // Store the listening addresses of the synthetic nodes so they can be set as initial peers of
    // the node.
    let mut listeners = Vec::with_capacity(test_messages.len());

    // Create a future for each message.
    let mut handles = Vec::with_capacity(test_messages.len());

    for message in test_messages {
        // Create a synthetic node and store its address.
        let mut synthetic_node = SyntheticNode::new(config.clone()).await.unwrap();
        listeners.push(synthetic_node.listening_addr());

        let handle = tokio::spawn(async move {
            // Receive Version.
            let (source, version) = synthetic_node.recv_message_timeout(TIMEOUT).await.unwrap();
            assert_matches!(version, Message::Version(..));

            // Respond with Version.
            synthetic_node
                .send_direct_message(
                    source,
                    Message::Version(Version::new(synthetic_node.listening_addr(), node_addr)),
                )
                .await
                .unwrap();

            // Receive Verack.
            let (_, verack) = synthetic_node.recv_message_timeout(TIMEOUT).await.unwrap();
            assert_matches!(verack, Message::Verack);

            // Send non-version.
            synthetic_node
                .send_direct_message(source, message)
                .await
                .unwrap();

            // Send Verack to complete the handshake.
            synthetic_node
                .send_direct_message(source, Message::Verack)
                .await
                .unwrap();

            // A ping/pong exchange indicates the node completed the handshake and ignored the
            // unsolicited message.
            synthetic_node.assert_ping_pong(source).await;

            // Gracefully shut down the synthetic node.
            synthetic_node.shut_down();
        });

        handles.push(handle);
    }

    // Start the node instance with the initial peers.
    node.initial_peers(listeners).start().await;

    // Run each future to completion.
    for handle in handles {
        handle.await.unwrap();
    }

    // Gracefully shut down the node.
    node.stop().await;
}

#[tokio::test]
async fn reject_version_reusing_nonce() {
    // ZG-CONFORMANCE-006
    //
    // The node rejects connections reusing its nonce (usually indicative of self-connection).
    //
    // zebra: closes the write half of the stream, doesn't close the socket.
    // zcashd: closes the write half of the stream, doesn't close the socket.

    // Create a synthetic node, no handshake, no message filters.
    let mut synthetic_node = SyntheticNode::new(SyntheticNodeConfig::default())
        .await
        .unwrap();

    // Spin up a node instance with the synthetic node set as an initial peer.
    let mut node: Node = Default::default();
    node.initial_peers(vec![synthetic_node.listening_addr()])
        .start()
        .await;

    // Receive a Version.
    let (source, version) = synthetic_node.recv_message_timeout(TIMEOUT).await.unwrap();
    let nonce = assert_matches!(version, Message::Version(version) => version.nonce);

    // Send a Version.
    let mut bad_version = Version::new(node.addr(), synthetic_node.listening_addr());
    bad_version.nonce = nonce;
    synthetic_node
        .send_direct_message(source, Message::Version(bad_version))
        .await
        .unwrap();

    // Assert on disconnect.
    wait_until!(TIMEOUT, synthetic_node.num_connected() == 0);

    // Gracefully shut down the nodes.
    synthetic_node.shut_down();
    node.stop().await;
}

#[tokio::test]
async fn reject_obsolete_versions() {
    // ZG-CONFORMANCE-007
    //
    // The node rejects connections with obsolete node versions.
    //
    // zebra: doesn't send a reject, closes the write half of the stream, doesn't close the socket.
    // zcashd: sends reject before closing the write half of the stream, doesn't close the socket.

    let obsolete_version_numbers: Vec<u32> = (170000..170002).collect();

    // Spin up a node instance.
    let mut node: Node = Default::default();
    node.initial_action(Action::WaitForConnection).start().await;

    // Configuration for all synthetic nodes, no handshake, no message filter.
    let config: SyntheticNodeConfig = Default::default();

    for obsolete_version_number in obsolete_version_numbers {
        // Create a synthetic node.
        let mut synthetic_node = SyntheticNode::new(config.clone()).await.unwrap();

        // Connect to the node and send a Version with an obsolete version.
        synthetic_node.connect(node.addr()).await.unwrap();
        synthetic_node
            .send_direct_message(
                node.addr(),
                Message::Version(
                    Version::new(node.addr(), synthetic_node.listening_addr())
                        .with_version(obsolete_version_number),
                ),
            )
            .await
            .unwrap();

        // Expect a reject message.
        let (_, reject) = synthetic_node.recv_message_timeout(TIMEOUT).await.unwrap();
        assert_matches!(reject, Message::Reject(reject) if reject.ccode == CCode::Obsolete);

        // Expect the connection to be dropped.
        wait_until!(TIMEOUT, synthetic_node.num_connected() == 0);

        // Gracefull shut down the synthetic node.
        synthetic_node.shut_down();
    }

    // Gracefully shut down the node.
    node.stop().await;
}
