//! Contains test cases which cover ZG-CONFORMANCE-005 and ZG-CONFORMANCE-006.
//!
//! The node ignores non-`Verack` message as a response to initial `Verack` it sent.

use std::io;

use crate::{
    protocol::{
        message::Message,
        payload::{
            block::{Block, LocatorHashes},
            Addr, Hash, Inv, Nonce,
        },
    },
    setup::node::{Action, Node},
    tools::{synthetic_node::SyntheticNode, LONG_TIMEOUT, RECV_TIMEOUT},
};

mod when_node_receives_connection {
    //! Contains test cases which cover ZG-CONFORMANCE-005.

    use super::*;

    #[tokio::test]
    async fn get_addr() {
        // zcashd: pass
        // zebra:  pass
        run_test_case(Message::GetAddr).await.unwrap();
    }

    #[tokio::test]
    async fn mempool() {
        // zcashd: pass
        // zebra:  pass
        run_test_case(Message::MemPool).await.unwrap();
    }

    #[tokio::test]
    async fn ping() {
        // zcashd: pass
        // zebra:  pass
        run_test_case(Message::Ping(Nonce::default()))
            .await
            .unwrap();
    }

    #[tokio::test]
    async fn pong() {
        // zcashd: pass
        // zebra:  pass
        run_test_case(Message::Pong(Nonce::default()))
            .await
            .unwrap();
    }

    #[tokio::test]
    async fn addr() {
        // zcashd: pass
        // zebra:  pass
        run_test_case(Message::Addr(Addr::empty())).await.unwrap();
    }

    #[tokio::test]
    async fn get_headers() {
        // zcashd: pass
        // zebra:  fail (disconnects)
        let block_hash = Block::testnet_genesis().double_sha256().unwrap();
        let block_loc = LocatorHashes::new(vec![block_hash], Hash::zeroed());
        run_test_case(Message::GetHeaders(block_loc)).await.unwrap();
    }

    #[tokio::test]
    async fn get_blocks() {
        // zcashd: pass
        // zebra:  fail (disconnects)
        let block_hash = Block::testnet_genesis().double_sha256().unwrap();
        let block_loc = LocatorHashes::new(vec![block_hash], Hash::zeroed());
        run_test_case(Message::GetBlocks(block_loc)).await.unwrap();
    }

    #[tokio::test]
    async fn get_data_block() {
        // zcashd: pass
        // zebra:  pass
        let block_inv = Inv::new(vec![Block::testnet_genesis().inv_hash()]);
        run_test_case(Message::GetData(block_inv)).await.unwrap();
    }

    #[tokio::test]
    async fn get_data_tx() {
        // zcashd: pass
        // zebra:  pass
        run_test_case(Message::GetData(Inv::new(vec![Block::testnet_genesis()
            .txs[0]
            .inv_hash()])))
        .await
        .unwrap();
    }

    #[tokio::test]
    async fn inv() {
        // zcashd: pass
        // zebra:  pass
        let block_inv = Inv::new(vec![Block::testnet_genesis().inv_hash()]);
        run_test_case(Message::Inv(block_inv)).await.unwrap();
    }

    #[tokio::test]
    async fn not_found() {
        // zcashd: pass
        // zebra:  pass
        let block_inv = Inv::new(vec![Block::testnet_genesis().inv_hash()]);
        run_test_case(Message::NotFound(block_inv)).await.unwrap();
    }

    /// Checks that `message` gets ignored when sent instead of [`Message::Version`] when the node
    /// receives the connection.
    async fn run_test_case(message: Message) -> io::Result<()> {
        // Spin up a node instance.
        let mut node = Node::new()?;
        node.initial_action(Action::WaitForConnection)
            .start()
            .await?;
        // Connect to the node, and exchange versions.
        let mut synthetic_node = SyntheticNode::builder()
            .with_version_exchange_handshake()
            .build()
            .await?;
        synthetic_node.connect(node.addr()).await?;

        // Send a non-verack message.
        synthetic_node.send_direct_message(node.addr(), message)?;

        // Expect the node to ignore the previous message, verify by completing the handshake.
        // Send Verack.
        synthetic_node.send_direct_message(node.addr(), Message::Verack)?;

        // Read Verack.
        match synthetic_node.recv_message_timeout(RECV_TIMEOUT).await {
            Ok((_, Message::Verack)) => Ok(()),
            Ok((_, unexpected)) => Err(io::Error::new(
                io::ErrorKind::Other,
                format!("Message was not ignored, received {}", unexpected),
            )),
            Err(_timeout) if !synthetic_node.is_connected(node.addr()) => Err(io::Error::new(
                io::ErrorKind::ConnectionAborted,
                "Connection terminated",
            )),
            Err(err) => Err(err),
        }?;

        // Gracefully shut down the nodes.
        synthetic_node.shut_down().await;
        node.stop()?;

        Ok(())
    }
}

mod when_node_initiates_connection {
    //! Contains test cases which cover ZG-CONFORMANCE-006.
    use super::*;

    #[tokio::test]
    async fn get_addr() {
        // zcashd: pass
        // zebra:  pass
        run_test_case(Message::GetAddr).await.unwrap();
    }

    #[tokio::test]
    async fn mempool() {
        // zcashd: pass
        // zebra:  pass
        run_test_case(Message::MemPool).await.unwrap();
    }

    #[tokio::test]
    async fn ping() {
        // zcashd: pass
        // zebra:  pass
        run_test_case(Message::Ping(Nonce::default()))
            .await
            .unwrap();
    }

    #[tokio::test]
    async fn pong() {
        // zcashd: pass
        // zebra:  pass
        run_test_case(Message::Pong(Nonce::default()))
            .await
            .unwrap();
    }

    #[tokio::test]
    async fn addr() {
        // zcashd: pass
        // zebra:  pass
        run_test_case(Message::Addr(Addr::empty())).await.unwrap();
    }

    #[tokio::test]
    async fn get_headers() {
        // zcashd: pass
        // zebra:  pass
        let block_hash = Block::testnet_genesis().double_sha256().unwrap();
        let block_loc = LocatorHashes::new(vec![block_hash], Hash::zeroed());
        run_test_case(Message::GetHeaders(block_loc)).await.unwrap();
    }

    #[tokio::test]
    async fn get_blocks() {
        // zcashd: pass
        // zebra:  pass
        let block_hash = Block::testnet_genesis().double_sha256().unwrap();
        let block_loc = LocatorHashes::new(vec![block_hash], Hash::zeroed());
        run_test_case(Message::GetBlocks(block_loc)).await.unwrap();
    }

    #[tokio::test]
    async fn get_data_block() {
        // zcashd: pass
        // zebra:  pass
        let block_inv = Inv::new(vec![Block::testnet_genesis().inv_hash()]);
        run_test_case(Message::GetData(block_inv)).await.unwrap();
    }

    #[tokio::test]
    async fn get_data_tx() {
        // zcashd: pass
        // zebra:  pass
        run_test_case(Message::GetData(Inv::new(vec![Block::testnet_genesis()
            .txs[0]
            .inv_hash()])))
        .await
        .unwrap();
    }

    #[tokio::test]
    async fn inv() {
        // zcashd: pass
        // zebra:  pass
        let block_inv = Inv::new(vec![Block::testnet_genesis().inv_hash()]);
        run_test_case(Message::Inv(block_inv)).await.unwrap();
    }

    #[tokio::test]
    async fn not_found() {
        // zcashd: pass
        // zebra:  pass
        let block_inv = Inv::new(vec![Block::testnet_genesis().inv_hash()]);
        run_test_case(Message::NotFound(block_inv)).await.unwrap();
    }

    /// Checks that `message` gets ignored when sent instead of [`Message::Verack`] when the node
    /// initiates the connection.
    async fn run_test_case(message: Message) -> io::Result<()> {
        // Create a SyntheticNode and store its listening address.
        // Enable version-only handshake
        let mut synthetic_node = SyntheticNode::builder()
            .with_version_exchange_handshake()
            .build()
            .await?;

        // Spin up a node instance which will connect to our SyntheticNode.
        let mut node = Node::new()?;
        node.initial_peers(vec![synthetic_node.listening_addr()])
            .start()
            .await?;

        // Wait for the node to establish the connection.
        // This will result in a connection in which the Version's have
        // already been exchanged.
        let node_addr =
            tokio::time::timeout(LONG_TIMEOUT, synthetic_node.wait_for_connection()).await?;

        // Send a non-version message.
        synthetic_node.send_direct_message(node_addr, message)?;

        // Expect the node to ignore the previous message, verify by completing the handshake.
        // Send Verack.
        synthetic_node.send_direct_message(node_addr, Message::Verack)?;

        // Read Verack.
        match synthetic_node.recv_message_timeout(RECV_TIMEOUT).await {
            Ok((_, Message::Verack)) => Ok(()),
            Ok((_, unexpected)) => Err(io::Error::new(
                io::ErrorKind::Other,
                format!("Message was not ignored, received {}", unexpected),
            )),
            Err(_timeout) if !synthetic_node.is_connected(node.addr()) => Err(io::Error::new(
                io::ErrorKind::ConnectionAborted,
                "Connection terminated",
            )),
            Err(err) => Err(err),
        }?;

        // Gracefully shut down the nodes.
        synthetic_node.shut_down().await;
        node.stop()?;

        Ok(())
    }
}
