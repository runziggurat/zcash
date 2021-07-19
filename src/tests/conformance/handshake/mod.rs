mod complete_handshake;
mod reject_version;

use crate::{
    protocol::{
        message::Message,
        payload::{
            block::{Block, LocatorHashes},
            Addr, Hash, Inv, Nonce, Version,
        },
    },
    setup::node::{Action, Node},
    tools::{synthetic_node::SyntheticNode, TIMEOUT},
};

use assert_matches::assert_matches;

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
    let mut node = Node::new().unwrap();
    node.initial_action(Action::WaitForConnection)
        .start()
        .await
        .unwrap();

    // Configuration to be used by all synthetic nodes, no handshaking, no message filters.
    let node_builder = SyntheticNode::builder();

    for message in test_messages {
        let mut synthetic_node = node_builder.build().await.unwrap();

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
    node.stop().unwrap();
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

    // Create peers (and get their listening address to pass to node)
    let (mut peers, addrs) = SyntheticNode::builder()
        .build_n(test_messages.len())
        .await
        .unwrap();
    // Instantiate a node instance without starting it (so we have access to its addr).
    let mut node = Node::new().unwrap();
    let node_addr = node.addr();

    // Create a future for each message.
    let mut handles = Vec::with_capacity(test_messages.len());

    for message in test_messages {
        let mut synthetic_node = peers.pop().unwrap();

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
    node.initial_peers(addrs).start().await.unwrap();

    // Run each future to completion.
    for handle in handles {
        handle.await.unwrap();
    }

    // Gracefully shut down the node.
    node.stop().unwrap();
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
    //
    // Create peers (and get their listening address to pass to node)
    let (mut peers, addrs) = SyntheticNode::builder()
        .with_all_auto_reply()
        .build_n(test_messages.len())
        .await
        .unwrap();

    // Instantiate a node instance without starting it (so we have access to its addr).
    let mut node = Node::new().unwrap();
    let node_addr = node.addr();

    // Create a future for each message.
    let mut handles = Vec::with_capacity(test_messages.len());

    for message in test_messages {
        let mut synthetic_node = peers.pop().unwrap();

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
            synthetic_node
                .ping_pong_timeout(source, TIMEOUT)
                .await
                .unwrap();

            // Gracefully shut down the synthetic node.
            synthetic_node.shut_down();
        });

        handles.push(handle);
    }

    // Start the node instance with the initial peers.
    node.initial_peers(addrs).start().await.unwrap();

    // Run each future to completion.
    for handle in handles {
        handle.await.unwrap();
    }

    // Gracefully shut down the node.
    node.stop().unwrap();
}
