//! Contains test cases which cover ZG-CONFORMANCE-005.
//!
//! The node ignores non-`Verack` message as a response to initial `Verack` it sent.

use std::time::Duration;

use crate::{
    protocol::{
        message::Message,
        payload::{
            block::{Block, LocatorHashes},
            Addr, Hash, Inv, Nonce,
        },
    },
    setup::node::Node,
    tools::synthetic_node::SyntheticNode,
};

use assert_matches::assert_matches;

const RECV_TIMEOUT: Duration = Duration::from_millis(100);
const CONNECTION_TIMEOUT: Duration = Duration::from_secs(10);

mod when_node_initiates_connection {
    //! Contains test cases which cover ZG-CONFORMANCE-005.
    use super::*;

    #[tokio::test]
    async fn get_addr() {
        // zcashd: pass
        // zebra:  pass
        run_test_case(Message::GetAddr).await;
    }

    #[tokio::test]
    async fn mempool() {
        // zcashd: pass
        // zebra:  fail (disconnects)
        run_test_case(Message::MemPool).await;
    }

    #[tokio::test]
    async fn ping() {
        // zcashd: pass
        // zebra:  fail (disconnects)
        run_test_case(Message::Ping(Nonce::default())).await;
    }

    #[tokio::test]
    async fn pong() {
        // zcashd: pass
        // zebra:  pass
        run_test_case(Message::Pong(Nonce::default())).await;
    }

    #[tokio::test]
    async fn addr() {
        // zcashd: pass
        // zebra:  pass
        run_test_case(Message::Addr(Addr::empty())).await;
    }

    #[tokio::test]
    async fn get_headers() {
        // zcashd: pass
        // zebra:  fail
        let block_hash = Block::testnet_genesis().double_sha256().unwrap();
        let block_loc = LocatorHashes::new(vec![block_hash], Hash::zeroed());
        run_test_case(Message::GetHeaders(block_loc)).await;
    }

    #[tokio::test]
    async fn get_blocks() {
        // zcashd: pass
        // zebra:  fail (disconnects)
        let block_hash = Block::testnet_genesis().double_sha256().unwrap();
        let block_loc = LocatorHashes::new(vec![block_hash], Hash::zeroed());
        run_test_case(Message::GetBlocks(block_loc)).await;
    }

    #[tokio::test]
    async fn get_data_block() {
        // zcashd: pass
        // zebra:  pass
        let block_inv = Inv::new(vec![Block::testnet_genesis().inv_hash()]);
        run_test_case(Message::GetData(block_inv)).await;
    }

    #[tokio::test]
    async fn get_data_tx() {
        // zcashd: pass
        // zebra:  pass
        run_test_case(Message::GetData(Inv::new(vec![Block::testnet_genesis()
            .txs[0]
            .inv_hash()])))
        .await;
    }

    #[tokio::test]
    async fn inv() {
        // zcashd: pass
        // zebra:  fail (disconnects)
        let block_inv = Inv::new(vec![Block::testnet_genesis().inv_hash()]);
        run_test_case(Message::Inv(block_inv)).await;
    }

    #[tokio::test]
    async fn not_found() {
        // zcashd: pass
        // zebra:  fail (disconnects)
        let block_inv = Inv::new(vec![Block::testnet_genesis().inv_hash()]);
        run_test_case(Message::NotFound(block_inv)).await;
    }

    /// Checks that `message` gets ignored when sent instead of [`Message::Verack`] when the node
    /// initiates the connection.
    async fn run_test_case(message: Message) {
        // Create a SyntheticNode and store its listening address.
        // Enable version-only handshake
        let mut synthetic_node = SyntheticNode::builder()
            .with_version_exchange_handshake()
            .build()
            .await
            .unwrap();

        // Spin up a node instance which will connect to our SyntheticNode.
        let mut node = Node::new().unwrap();
        node.initial_peers(vec![synthetic_node.listening_addr()])
            .start()
            .await
            .unwrap();

        // Wait for the node to establish the connection.
        // This will result in a connection in which the Version's have
        // already been exchanged.
        let node_addr =
            tokio::time::timeout(CONNECTION_TIMEOUT, synthetic_node.wait_for_connection())
                .await
                .expect("Timeout waiting for node to establish connection");

        // Send a non-version message.
        synthetic_node
            .send_direct_message(node_addr, message)
            .await
            .expect("Sending non-version message");

        // Expect the node to ignore the previous message, verify by completing the handshake.
        // Send Verack.
        synthetic_node
            .send_direct_message(node_addr, Message::Verack)
            .await
            .expect("Sending Verack");

        // Read Verack.
        let (_, verack) = synthetic_node
            .recv_message_timeout(RECV_TIMEOUT)
            .await
            .expect("Receiving Verack");
        assert_matches!(verack, Message::Verack);

        // Gracefully shut down the nodes.
        synthetic_node.shut_down();
        node.stop().unwrap();
    }
}
