//! Contains test cases which cover ZG-CONFORMANCE-003 and ZG-CONFORMANCE-004.
//!
//! The node ignores non-version messages sent inplace of version.

use crate::{
    protocol::{
        message::Message,
        payload::{
            block::{Block, LocatorHashes},
            Addr, Hash, Inv, Nonce, Version,
        },
    },
    setup::node::{Action, Node},
    tools::synthetic_node::SyntheticNode,
};

use std::{io, time::Duration};

const RECV_TIMEOUT: Duration = Duration::from_millis(100);
const CONNECTION_TIMEOUT: Duration = Duration::from_secs(5);

mod when_node_receives_connection {
    //! Contains test cases which cover ZG-CONFORMANCE-003.

    use super::*;

    #[tokio::test]
    async fn get_addr() {
        // zcashd: pass
        // zebra:  fail (disconnects)
        run_test_case(Message::GetAddr).await.unwrap();
    }

    #[tokio::test]
    async fn mempool() {
        // zcashd: pass
        // zebra:  fail (disconnects)
        run_test_case(Message::MemPool).await.unwrap();
    }

    #[tokio::test]
    async fn verack() {
        // zcashd: pass
        // zebra:  fail (disconnects)
        run_test_case(Message::Verack).await.unwrap();
    }

    #[tokio::test]
    async fn ping() {
        // zcashd: pass
        // zebra:  fail (disconnects)
        run_test_case(Message::Ping(Nonce::default()))
            .await
            .unwrap();
    }

    #[tokio::test]
    async fn pong() {
        // zcashd: pass
        // zebra:  fail (disconnects)
        run_test_case(Message::Pong(Nonce::default()))
            .await
            .unwrap();
    }

    #[tokio::test]
    async fn addr() {
        // zcashd: pass
        // zebra:  fail (disconnects)
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
        // zebra:  fail (disconnects)
        let block_inv = Inv::new(vec![Block::testnet_genesis().inv_hash()]);
        run_test_case(Message::GetData(block_inv)).await.unwrap();
    }

    #[tokio::test]
    async fn get_data_tx() {
        // zcashd: pass
        // zebra:  fail (disconnects)
        run_test_case(Message::GetData(Inv::new(vec![Block::testnet_genesis()
            .txs[0]
            .inv_hash()])))
        .await
        .unwrap();
    }

    #[tokio::test]
    async fn inv() {
        // zcashd: pass
        // zebra:  fail (disconnects)
        let block_inv = Inv::new(vec![Block::testnet_genesis().inv_hash()]);
        run_test_case(Message::Inv(block_inv)).await.unwrap();
    }

    #[tokio::test]
    async fn not_found() {
        // zcashd: pass
        // zebra:  fail (disconnects)
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
        // Connect to the node, don't handshake.
        let mut synthetic_node = SyntheticNode::builder().build().await?;
        synthetic_node.connect(node.addr()).await?;

        // Send a non-version message.
        synthetic_node
            .send_direct_message(node.addr(), message)
            .await?;

        // Expect the node to ignore the previous message, verify by completing the handshake.
        // Send Version.
        synthetic_node
            .send_direct_message(
                node.addr(),
                Message::Version(Version::new(synthetic_node.listening_addr(), node.addr())),
            )
            .await?;

        // Read Version.
        match synthetic_node.recv_message_timeout(RECV_TIMEOUT).await {
            Ok((_, Message::Version(..))) => Ok(()),
            Ok((_, unexpected)) => Err(io::Error::new(
                io::ErrorKind::Other,
                format!(
                    "Message was not ignored. Instead of Version received {}",
                    unexpected
                ),
            )),
            Err(_timeout) if !synthetic_node.is_connected(node.addr()) => Err(io::Error::new(
                io::ErrorKind::ConnectionAborted,
                "Connection terminated",
            )),
            Err(err) => Err(err),
        }?;

        // Send Verack.
        synthetic_node
            .send_direct_message(node.addr(), Message::Verack)
            .await?;

        // Read Verack.
        match synthetic_node.recv_message_timeout(RECV_TIMEOUT).await {
            Ok((_, Message::Verack)) => Ok(()),
            Ok((_, unexpected)) => Err(io::Error::new(
                io::ErrorKind::Other,
                format!(
                    "Message was not ignored. Instead of Verack received {}",
                    unexpected
                ),
            )),
            Err(_timeout) if !synthetic_node.is_connected(node.addr()) => Err(io::Error::new(
                io::ErrorKind::ConnectionAborted,
                "Connection terminated",
            )),
            Err(err) => Err(err),
        }?;

        // Gracefully shut down the nodes.
        synthetic_node.shut_down();
        node.stop()?;

        Ok(())
    }
}

mod when_node_initiates_connection {
    //! Contains test cases which cover ZG-CONFORMANCE-004.
    use super::*;

    #[tokio::test]
    async fn get_addr() {
        // zcashd: pass
        // zebra:  fail (disconnects)
        run_test_case(Message::GetAddr).await.unwrap();
    }

    #[tokio::test]
    async fn mempool() {
        // zcashd: pass
        // zebra:  fail (disconnects)
        run_test_case(Message::MemPool).await.unwrap();
    }

    #[tokio::test]
    async fn verack() {
        // zcashd: pass
        // zebra:  fail (disconnects)
        run_test_case(Message::Verack).await.unwrap();
    }

    #[tokio::test]
    async fn ping() {
        // zcashd: pass
        // zebra:  fail (disconnects)
        run_test_case(Message::Ping(Nonce::default()))
            .await
            .unwrap();
    }

    #[tokio::test]
    async fn pong() {
        // zcashd: pass
        // zebra:  fail (disconnects)
        run_test_case(Message::Pong(Nonce::default()))
            .await
            .unwrap();
    }

    #[tokio::test]
    async fn addr() {
        // zcashd: pass
        // zebra:  fail (disconnects)
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
        // zebra:  fail (disconnects)
        let block_inv = Inv::new(vec![Block::testnet_genesis().inv_hash()]);
        run_test_case(Message::GetData(block_inv)).await.unwrap();
    }

    #[tokio::test]
    async fn get_data_tx() {
        // zcashd: pass
        // zebra:  fail (disconnects)
        run_test_case(Message::GetData(Inv::new(vec![Block::testnet_genesis()
            .txs[0]
            .inv_hash()])))
        .await
        .unwrap();
    }

    #[tokio::test]
    async fn inv() {
        // zcashd: pass
        // zebra:  fail (disconnects)
        let block_inv = Inv::new(vec![Block::testnet_genesis().inv_hash()]);
        run_test_case(Message::Inv(block_inv)).await.unwrap();
    }

    #[tokio::test]
    async fn not_found() {
        // zcashd: pass
        // zebra:  fail (disconnects)
        let block_inv = Inv::new(vec![Block::testnet_genesis().inv_hash()]);
        run_test_case(Message::NotFound(block_inv)).await.unwrap();
    }

    /// Checks that `message` gets ignored when sent instead of [`Message::Version`] when the node
    /// initiates the connection.
    async fn run_test_case(message: Message) -> io::Result<()> {
        // Create a SyntheticNode and store its listening address.
        let mut synthetic_node = SyntheticNode::builder().build().await?;

        // Spin up a node instance which will connect to our SyntheticNode.
        let mut node = Node::new()?;
        node.initial_peers(vec![dbg!(synthetic_node.listening_addr())])
            .start()
            .await?;

        // Wait for the node to establish the connection.
        let node_addr =
            tokio::time::timeout(CONNECTION_TIMEOUT, synthetic_node.wait_for_connection()).await?;

        // Send a non-version message.
        synthetic_node
            .send_direct_message(node_addr, message)
            .await?;

        // Expect the node to ignore the previous message, verify by completing the handshake.
        // Send Version.
        synthetic_node
            .send_direct_message(
                node_addr,
                Message::Version(Version::new(synthetic_node.listening_addr(), node_addr)),
            )
            .await?;

        // Read Version.
        match synthetic_node.recv_message_timeout(RECV_TIMEOUT).await {
            Ok((_, Message::Version(..))) => Ok(()),
            Ok((_, unexpected)) => Err(io::Error::new(
                io::ErrorKind::Other,
                format!(
                    "Message was not ignored. Instead of Version received {}",
                    unexpected
                ),
            )),
            Err(_timeout) if !synthetic_node.is_connected(node.addr()) => Err(io::Error::new(
                io::ErrorKind::ConnectionAborted,
                "Connection terminated",
            )),
            Err(err) => Err(err),
        }?;

        // Send Verack.
        synthetic_node
            .send_direct_message(node_addr, Message::Verack)
            .await?;

        // Read Verack.
        match synthetic_node.recv_message_timeout(RECV_TIMEOUT).await {
            Ok((_, Message::Verack)) => Ok(()),
            Ok((_, unexpected)) => Err(io::Error::new(
                io::ErrorKind::Other,
                format!(
                    "Message was not ignored. Instead of Verack received {}",
                    unexpected
                ),
            )),
            Err(_timeout) if !synthetic_node.is_connected(node.addr()) => Err(io::Error::new(
                io::ErrorKind::ConnectionAborted,
                "Connection terminated",
            )),
            Err(err) => Err(err),
        }?;

        // Gracefully shut down the nodes.
        synthetic_node.shut_down();
        node.stop()?;

        Ok(())
    }
}
