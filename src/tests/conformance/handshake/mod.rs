mod complete_handshake;
mod ignore_message_inplace_of_version;
mod reject_version;

use crate::{
    protocol::{
        message::Message,
        payload::{
            block::{Block, LocatorHashes},
            Addr, Hash, Inv, Nonce, Version,
        },
    },
    setup::node::Node,
    tools::{synthetic_node::SyntheticNode, TIMEOUT},
};

use assert_matches::assert_matches;

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
