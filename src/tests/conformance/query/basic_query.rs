//! Contains test cases which cover ZG-CONFORMANCE-011
//!
//! The node responds with the correct messages. Message correctness is naively verified through successful encoding/decoding.
//!
//! This is tested under two different scenarios:
//!
//!     1. the node is seeded with the initial testnet blocks.
//!     2. the node is *not* seeded with data at all.
//!
//! Queries and expected replies:
//!
//!     - Ping           -> Pong
//!     - GetAddr        -> Addr
//!     - Mempool        -> Inv
//!     - GetBlocks      -> Inv
//!     - GetHeaders     -> Headers
//!     - GetData(block) -> Block (1) | NotFound (2)
//!     - GetData(tx)    -> Tx    (1) | NotFound (2)

use crate::{
    protocol::{
        message::Message,
        payload::{
            block::{Block, LocatorHashes},
            Hash, Inv, Nonce,
        },
    },
    setup::node::{Action, Node},
    tools::synthetic_node::{PingPongError, SyntheticNode},
};
use assert_matches::assert_matches;
use std::{io, time::Duration};

const RECV_TIMEOUT: Duration = Duration::from_millis(100);

mod node_is_seeded_with_blocks {
    use super::*;

    #[tokio::test]
    async fn ping() {
        // zcashd: pass
        // zebra:  fail (seeding not supported for zebra)
        let nonce = Nonce::default();
        let expected = Message::Pong(nonce);
        let reply = run_test_case(Message::Ping(nonce)).await.unwrap();
        assert_eq!(reply, expected);
    }

    #[tokio::test]
    async fn get_addr() {
        // zcashd: fail (query ignored)
        // zebra:  fail (seeding not supported for zebra)
        let reply = run_test_case(Message::GetAddr).await.unwrap();
        assert_matches!(reply, Message::Addr(..));
    }

    #[tokio::test]
    async fn mempool() {
        // zcashd: fail (query ignored)
        // zebra:  fail (seeding not supported for zebra)
        let reply = run_test_case(Message::MemPool).await.unwrap();
        assert_matches!(reply, Message::Inv(..));
    }

    #[tokio::test]
    async fn get_blocks() {
        // zcashd: pass
        // zebra:  fail (seeding not supported for zebra)
        let query = Message::GetBlocks(LocatorHashes::new(
            vec![Block::testnet_genesis().double_sha256().unwrap()],
            Hash::zeroed(),
        ));
        let reply = run_test_case(query).await.unwrap();
        assert_matches!(reply, Message::Inv(..));
    }

    #[tokio::test]
    async fn get_data_block() {
        // zcashd: pass
        // zebra:  fail (seeding not supported for zebra)
        let query = Message::GetData(Inv::new(vec![Block::testnet_2().inv_hash()]));
        let reply = run_test_case(query).await.unwrap();
        assert_matches!(reply, Message::Block(..));
    }

    #[tokio::test]
    // This test should currently fail, since we have no way of seeding the Mempool of the node.
    async fn get_data_tx() {
        // zcashd: fail (NotFound), this is expected as we cannot seed the mempool of the node.
        // zebra:  fail (seeding not supported for zebra)
        let query = Message::GetData(Inv::new(vec![Block::testnet_genesis().txs[0].inv_hash()]));
        let reply = run_test_case(query).await.unwrap();
        assert_matches!(reply, Message::Tx(..));
    }

    #[tokio::test]
    async fn get_headers() {
        // zcashd: pass
        // zebra:  fail (seeding not supported for zebra)
        let query = Message::GetHeaders(LocatorHashes::new(
            vec![Block::testnet_genesis().double_sha256().unwrap()],
            Hash::zeroed(),
        ));
        let reply = run_test_case(query).await.unwrap();
        assert_matches!(reply, Message::Headers(..));
    }

    async fn run_test_case(query: Message) -> io::Result<Message> {
        // Spin up a seeded node instance.
        let mut node = Node::new().unwrap();
        node.initial_action(Action::SeedWithTestnetBlocks(11))
            .start()
            .await?;

        // Create a synthetic node.
        let mut synthetic_node = SyntheticNode::builder()
            .with_full_handshake()
            .with_all_auto_reply()
            .build()
            .await?;

        // Connect to the node and initiate handshake.
        synthetic_node.connect(node.addr()).await?;

        // Send the query.
        synthetic_node.send_direct_message(node.addr(), query)?;

        // Use Ping-Pong to check node's response.
        let result = match synthetic_node
            .ping_pong_timeout(node.addr(), RECV_TIMEOUT)
            .await
        {
            Ok(_) => Err(io::Error::new(io::ErrorKind::Other, "Query was ignored")),
            Err(PingPongError::Unexpected(msg)) => Ok(*msg),
            Err(err) => Err(err.into()),
        };

        // Gracefully shut down the nodes.
        synthetic_node.shut_down().await;
        node.stop()?;

        result
    }
}

mod node_is_not_seeded_with_blocks {
    use super::*;

    #[tokio::test]
    async fn ping() {
        // zcashd: pass
        // zebra:  pass
        let nonce = Nonce::default();
        let expected = Message::Pong(nonce);
        let reply = run_test_case(Message::Ping(nonce)).await.unwrap();
        assert_eq!(reply, expected);
    }

    #[tokio::test]
    async fn get_addr() {
        // zcashd: fail (query ignored)
        // zebra:  fail (query ignored, nil response internally)
        let reply = run_test_case(Message::GetAddr).await.unwrap();
        assert_matches!(reply, Message::Addr(..));
    }

    #[tokio::test]
    async fn mempool() {
        // zcashd: fail (query ignored)
        // zebra:  fail (query ignored)
        let reply = run_test_case(Message::MemPool).await.unwrap();
        assert_matches!(reply, Message::Inv(..));
    }

    #[tokio::test]
    async fn get_blocks() {
        // zcashd: fail (query ignored)
        // zebra:  fail (query ignored)
        let query = Message::GetBlocks(LocatorHashes::new(
            vec![Block::testnet_genesis().double_sha256().unwrap()],
            Hash::zeroed(),
        ));
        let reply = run_test_case(query).await.unwrap();
        assert_matches!(reply, Message::Inv(..));
    }

    #[tokio::test]
    async fn get_data_block() {
        // zcashd: fail (query ignored)
        // zebra:  fail (query ignored), pass when timeout is used to account for node startup (10s
        // is usually enough).
        let inv = Inv::new(vec![Block::testnet_2().inv_hash()]);
        let query = Message::GetData(inv.clone());
        let expected = Message::NotFound(inv);
        let reply = run_test_case(query).await.unwrap();
        assert_eq!(reply, expected);
    }

    #[tokio::test]
    async fn get_data_tx() {
        // zcashd: pass
        // zebra:  fail (query ignored), sometimes pass (flaky), reliably passes when timeout is
        // used to account for node startup (10s is usually enough).
        let inv = Inv::new(vec![Block::testnet_genesis().txs[0].inv_hash()]);
        let query = Message::GetData(inv.clone());
        let expected = Message::NotFound(inv);
        let reply = run_test_case(query).await.unwrap();
        assert_eq!(reply, expected);
    }

    #[tokio::test]
    async fn get_headers() {
        // zcashd: pass
        // zebra:  fail (query ignored)
        let query = Message::GetHeaders(LocatorHashes::new(
            vec![Block::testnet_genesis().double_sha256().unwrap()],
            Hash::zeroed(),
        ));
        let reply = run_test_case(query).await.unwrap();
        assert_matches!(reply, Message::Headers(..));
    }

    async fn run_test_case(query: Message) -> io::Result<Message> {
        // Spin up a non-seeded node instance.
        let mut node = Node::new().unwrap();
        node.initial_action(Action::WaitForConnection)
            .start()
            .await?;

        // Create a synthetic node.
        let mut synthetic_node = SyntheticNode::builder()
            .with_full_handshake()
            .with_all_auto_reply()
            .build()
            .await?;

        // Connect to the node and initiate handshake.
        synthetic_node.connect(node.addr()).await?;

        // Send the query.
        synthetic_node.send_direct_message(node.addr(), query)?;

        // Use Ping-Pong to check node's response.
        let result = match synthetic_node
            .ping_pong_timeout(node.addr(), RECV_TIMEOUT)
            .await
        {
            Ok(_) => Err(io::Error::new(io::ErrorKind::Other, "Query was ignored")),
            Err(PingPongError::Unexpected(msg)) => Ok(*msg),
            Err(err) => Err(err.into()),
        };

        // Gracefully shut down the nodes.
        synthetic_node.shut_down().await;
        node.stop()?;

        result
    }
}
