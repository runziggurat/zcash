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

use std::io;

use assert_matches::assert_matches;

use crate::{
    protocol::{
        message::Message,
        payload::{
            block::{Block, LocatorHashes},
            Hash, Inv, Nonce,
        },
    },
    setup::node::{Action, Node},
    tools::{
        synthetic_node::{PingPongError, SyntheticNode},
        RECV_TIMEOUT,
    },
};

mod node_is_seeded_with_blocks {
    use super::*;

    #[tokio::test]
    #[allow(non_snake_case)]
    async fn c011_t1_PING() {
        // zcashd: pass
        // zebra:  fail (seeding not supported for zebra)
        let nonce = Nonce::default();
        let expected = Message::Pong(nonce);
        let reply = run_test_case(Message::Ping(nonce)).await.unwrap();
        assert_eq!(reply, expected);
    }

    #[tokio::test]
    #[allow(non_snake_case)]
    async fn c011_t2_GET_ADDR() {
        // zcashd: fail (query ignored)
        // zebra:  fail (seeding not supported for zebra)
        let reply = run_test_case(Message::GetAddr).await.unwrap();
        assert_matches!(reply, Message::Addr(..));
    }

    #[tokio::test]
    #[allow(non_snake_case)]
    async fn c011_t3_MEMPOOL() {
        // zcashd: fail (query ignored)
        // zebra:  fail (seeding not supported for zebra)
        let reply = run_test_case(Message::MemPool).await.unwrap();
        assert_matches!(reply, Message::Inv(..));
    }

    #[tokio::test]
    #[allow(non_snake_case)]
    async fn c011_t4_GET_BLOCKS() {
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
    #[allow(non_snake_case)]
    async fn c011_t5_GET_DATA_BLOCK() {
        // zcashd: pass
        // zebra:  fail (seeding not supported for zebra)
        let query = Message::GetData(Inv::new(vec![Block::testnet_2().inv_hash()]));
        let reply = run_test_case(query).await.unwrap();
        assert_matches!(reply, Message::Block(..));
    }

    #[tokio::test]
    #[allow(non_snake_case)]
    // This test should currently fail, since we have no way of seeding the Mempool of the node.
    async fn c011_t6_GET_DATA_TX() {
        // zcashd: fail (NotFound), this is expected as we cannot seed the mempool of the node.
        // zebra:  fail (seeding not supported for zebra)
        let query = Message::GetData(Inv::new(vec![Block::testnet_genesis().txs[0].inv_hash()]));
        let reply = run_test_case(query).await.unwrap();
        assert_matches!(reply, Message::Tx(..));
    }

    #[tokio::test]
    #[allow(non_snake_case)]
    async fn c011_t7_GET_HEADERS() {
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
        synthetic_node.unicast(node.addr(), query)?;

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
    #[allow(non_snake_case)]
    async fn c011_t8_PING() {
        // zcashd: pass
        // zebra:  pass
        let nonce = Nonce::default();
        let expected = Message::Pong(nonce);
        let reply = run_test_case(Message::Ping(nonce)).await.unwrap();
        assert_eq!(reply, expected);
    }

    #[tokio::test]
    #[allow(non_snake_case)]
    async fn c011_t9_GET_ADDR() {
        // zcashd: fail (query ignored)
        // zebra:  fail (query ignored, nil response internally)
        let reply = run_test_case(Message::GetAddr).await.unwrap();
        assert_matches!(reply, Message::Addr(..));
    }

    #[tokio::test]
    #[allow(non_snake_case)]
    async fn c011_t10_MEMPOOL() {
        // zcashd: fail (query ignored)
        // zebra:  fail (query ignored)
        let reply = run_test_case(Message::MemPool).await.unwrap();
        assert_matches!(reply, Message::Inv(..));
    }

    #[tokio::test]
    #[allow(non_snake_case)]
    async fn c011_t11_GET_BLOCKS() {
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
    #[allow(non_snake_case)]
    async fn c011_t12_GET_DATA_BLOCK() {
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
    #[allow(non_snake_case)]
    async fn c011_t13_GET_DATA_TX() {
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
    #[allow(non_snake_case)]
    async fn c011_t14_GET_HEADERS() {
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
        synthetic_node.unicast(node.addr(), query)?;

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
