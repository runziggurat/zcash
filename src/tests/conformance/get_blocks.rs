//! Contains test cases which cover ZG-CONFORMANCE-015
//!
//! The node responds to `GetBlocks` requests with a list of blocks based on the provided range.
//!
//! Three broad categories are tested.
//!  1. no-range limit (stop_hash = [0]).
//!  2. stop_hash == start_hash (i.e. the range should be zero).
//!  3. ranged queries (stop_hash is valid).
//!
//! Note: Zebra does not support seeding with chain data and as such cannot run any of these tests successfully.
//!
//! Note: ZCashd does not fully follow the bitcoin spec. The spec states that the stop_hash should be included in
//!       the range returned by the node. ZCashd excludes it. We are taking ZCashd's behaviour as correct.
//!
//! Note: ZCashd ignores queries for which it would have replied with an empty range. We are taking this behaviour
//!       as correct. A more well-formed response would be an empty list.

use std::{io, time::Duration};

use crate::{
    protocol::{
        message::Message,
        payload::{
            block::{Block, LocatorHashes},
            inv::InvHash,
            Hash, Inv,
        },
    },
    setup::node::{Action, Node},
    tools::synthetic_node::{PingPongError, SyntheticNode},
};

use assert_matches::assert_matches;

lazy_static::lazy_static!(
    /// The blocks that the node is seeded with for this test module.
    static ref SEED_BLOCKS: Vec<Block> = {
        Block::initial_testnet_blocks()
    };

    /// InvHashes of the blocks that the node is seeded with.
    static ref SEED_BLOCK_HASHES: Vec<InvHash> = {
        SEED_BLOCKS.iter().map(|block| block.inv_hash()).collect()
    };
);

mod stop_hash_is_zero {
    //! No range limit tests (stop_hash = [0]).
    use super::*;

    #[tokio::test]
    async fn from_block_0_onwards() {
        // zcashd: pass
        const BLOCK_INDEX: usize = 0;
        let query = Message::GetBlocks(LocatorHashes::new(
            vec![SEED_BLOCKS[BLOCK_INDEX].double_sha256().unwrap()],
            Hash::zeroed(),
        ));

        let expected = Message::Inv(Inv::new(SEED_BLOCK_HASHES[BLOCK_INDEX + 1..].to_owned()));
        let result = run_test_case(query).await.unwrap();
        assert_matches!(result, QueryResult::Reply(msg) if *msg == expected);
    }

    #[tokio::test]
    async fn from_block_1_onwards() {
        // zcashd: pass
        const BLOCK_INDEX: usize = 1;
        let query = Message::GetBlocks(LocatorHashes::new(
            vec![SEED_BLOCKS[BLOCK_INDEX].double_sha256().unwrap()],
            Hash::zeroed(),
        ));

        let expected = Message::Inv(Inv::new(SEED_BLOCK_HASHES[BLOCK_INDEX + 1..].to_owned()));
        let result = run_test_case(query).await.unwrap();
        assert_matches!(result, QueryResult::Reply(msg) if *msg == expected);
    }

    #[tokio::test]
    async fn from_block_5_onwards() {
        // zcashd: pass
        const BLOCK_INDEX: usize = 5;
        let query = Message::GetBlocks(LocatorHashes::new(
            vec![SEED_BLOCKS[BLOCK_INDEX].double_sha256().unwrap()],
            Hash::zeroed(),
        ));

        let expected = Message::Inv(Inv::new(SEED_BLOCK_HASHES[BLOCK_INDEX + 1..].to_owned()));
        let result = run_test_case(query).await.unwrap();
        assert_matches!(result, QueryResult::Reply(msg) if *msg == expected);
    }

    #[tokio::test]
    async fn from_penultimate_block_onwards() {
        // zcashd: pass
        let block_index: usize = SEED_BLOCKS.len() - 2;
        let query = Message::GetBlocks(LocatorHashes::new(
            vec![SEED_BLOCKS[block_index].double_sha256().unwrap()],
            Hash::zeroed(),
        ));
        let expected = Message::Inv(Inv::new(SEED_BLOCK_HASHES[block_index + 1..].to_owned()));

        let result = run_test_case(query).await.unwrap();
        assert_matches!(result, QueryResult::Reply(msg) if *msg == expected);
    }

    #[tokio::test]
    async fn final_block_is_ignored() {
        // Test that we get no response for the final block in the known-chain
        // (this is the behaviour exhibited by zcashd - a more well-formed response
        // might be sending an empty inventory instead).
        //
        // zcashd: pass
        let block_index: usize = SEED_BLOCKS.len() - 1;
        let query = Message::GetBlocks(LocatorHashes::new(
            vec![SEED_BLOCKS[block_index].double_sha256().unwrap()],
            Hash::zeroed(),
        ));

        let result = run_test_case(query).await.unwrap();
        assert_matches!(result, QueryResult::Ignored);
    }

    #[tokio::test]
    async fn out_of_order() {
        // Node expects the block hashes in reverse order, i.e. newest first.
        // It should latch onto the first known hash and ignore the rest.
        // In this test we swop the hash order and expect it to ignore the
        // second hash.
        //
        // zcashd: pass
        let block_index: usize = 7;
        let query = Message::GetBlocks(LocatorHashes::new(
            vec![
                SEED_BLOCKS[block_index].double_sha256().unwrap(),
                SEED_BLOCKS[block_index + 1].double_sha256().unwrap(),
            ],
            Hash::zeroed(),
        ));

        let expected = Message::Inv(Inv::new(SEED_BLOCK_HASHES[block_index + 1..].to_owned()));

        let result = run_test_case(query).await.unwrap();
        assert_matches!(result, QueryResult::Reply(msg) if *msg == expected);
    }

    #[tokio::test]
    async fn off_chain_correction() {
        // Test that we get corrected if we are "off chain".
        // We expect that unknown hashes get ignored, until it finds a known hash; it then returns
        // all known children of that block.
        //
        // zcashd: pass
        const BLOCK_INDEX: usize = 1;

        let query = Message::GetBlocks(LocatorHashes::new(
            vec![
                Hash::new([19; 32]),
                Hash::new([22; 32]),
                SEED_BLOCKS[BLOCK_INDEX].double_sha256().unwrap(),
            ],
            Hash::zeroed(),
        ));

        let result = run_test_case(query).await.unwrap();
        let expected = Message::Inv(Inv::new(SEED_BLOCK_HASHES[BLOCK_INDEX + 1..].to_owned()));

        assert_matches!(result, QueryResult::Reply(msg) if *msg == expected);
    }
}

mod stop_hash_is_start_hash {
    use super::*;

    #[tokio::test]
    async fn from_block_0_to_0() {
        // zcashd: fail (sends all blocks[1+] - same behaviour as if query was not range limited)
        const BLOCK_RANGE: (usize, usize) = (0, 0);
        let query = Message::GetBlocks(LocatorHashes::new(
            vec![SEED_BLOCKS[BLOCK_RANGE.0].double_sha256().unwrap()],
            SEED_BLOCKS[BLOCK_RANGE.1].double_sha256().unwrap(),
        ));

        let result = run_test_case(query).await.unwrap();

        assert_matches!(result, QueryResult::Ignored);
    }

    #[tokio::test]
    async fn from_block_4_to_4() {
        // zcashd: fail (sends all blocks[5+] - same behaviour as if query was not range limited)
        const BLOCK_RANGE: (usize, usize) = (4, 4);
        let query = Message::GetBlocks(LocatorHashes::new(
            vec![SEED_BLOCKS[BLOCK_RANGE.0].double_sha256().unwrap()],
            SEED_BLOCKS[BLOCK_RANGE.1].double_sha256().unwrap(),
        ));

        let result = run_test_case(query).await.unwrap();

        assert_matches!(result, QueryResult::Ignored);
    }

    #[tokio::test]
    async fn from_final_block_to_final_block() {
        // zcashd: pass
        let block_range: (usize, usize) = (SEED_BLOCKS.len() - 1, SEED_BLOCKS.len() - 1);
        let query = Message::GetBlocks(LocatorHashes::new(
            vec![SEED_BLOCKS[block_range.0].double_sha256().unwrap()],
            SEED_BLOCKS[block_range.1].double_sha256().unwrap(),
        ));

        let result = run_test_case(query).await.unwrap();

        assert_matches!(result, QueryResult::Ignored);
    }
}

mod ranged {
    use super::*;

    #[tokio::test]
    async fn from_block_0_to_1() {
        // zcashd: pass
        const BLOCK_RANGE: (usize, usize) = (0, 1);
        let query = Message::GetBlocks(LocatorHashes::new(
            vec![SEED_BLOCKS[BLOCK_RANGE.0].double_sha256().unwrap()],
            SEED_BLOCKS[BLOCK_RANGE.1].double_sha256().unwrap(),
        ));

        let result = run_test_case(query).await.unwrap();
        assert_matches!(result, QueryResult::Ignored);
    }

    #[tokio::test]
    async fn from_block_0_to_5() {
        // zcashd: pass
        const BLOCK_RANGE: (usize, usize) = (0, 5);
        let query = Message::GetBlocks(LocatorHashes::new(
            vec![SEED_BLOCKS[BLOCK_RANGE.0].double_sha256().unwrap()],
            SEED_BLOCKS[BLOCK_RANGE.1].double_sha256().unwrap(),
        ));

        let expected = Message::Inv(Inv::new(
            SEED_BLOCK_HASHES[BLOCK_RANGE.0 + 1..BLOCK_RANGE.1].to_owned(),
        ));
        let result = run_test_case(query).await.unwrap();
        assert_matches!(result, QueryResult::Reply(msg) if *msg == expected);
    }

    #[tokio::test]
    async fn from_block_0_to_final_block() {
        // zcashd: pass
        let block_range: (usize, usize) = (0, SEED_BLOCKS.len() - 1);
        let query = Message::GetBlocks(LocatorHashes::new(
            vec![SEED_BLOCKS[block_range.0].double_sha256().unwrap()],
            SEED_BLOCKS[block_range.1].double_sha256().unwrap(),
        ));

        let expected = Message::Inv(Inv::new(
            SEED_BLOCK_HASHES[block_range.0 + 1..block_range.1].to_owned(),
        ));
        let result = run_test_case(query).await.unwrap();
        assert_matches!(result, QueryResult::Reply(msg) if *msg == expected);
    }

    #[tokio::test]
    async fn from_block_3_to_9() {
        // zcashd: pass
        const BLOCK_RANGE: (usize, usize) = (3, 9);
        let query = Message::GetBlocks(LocatorHashes::new(
            vec![SEED_BLOCKS[BLOCK_RANGE.0].double_sha256().unwrap()],
            SEED_BLOCKS[BLOCK_RANGE.1].double_sha256().unwrap(),
        ));

        let expected = Message::Inv(Inv::new(
            SEED_BLOCK_HASHES[BLOCK_RANGE.0 + 1..BLOCK_RANGE.1].to_owned(),
        ));
        let result = run_test_case(query).await.unwrap();
        assert_matches!(result, QueryResult::Reply(msg) if *msg == expected);
    }

    #[tokio::test]
    async fn from_penultimate_block_to_final_block() {
        // zcashd: pass
        let block_index: usize = SEED_BLOCKS.len() - 2;
        let query = Message::GetBlocks(LocatorHashes::new(
            vec![SEED_BLOCKS[block_index].double_sha256().unwrap()],
            SEED_BLOCKS.last().unwrap().double_sha256().unwrap(),
        ));

        let result = run_test_case(query).await.unwrap();
        assert_matches!(result, QueryResult::Ignored);
    }

    #[tokio::test]
    async fn off_chain_correction() {
        // Test that we get corrected if we are "off chain".
        // We expect that unknown hashes get ignored, until it finds a known hash; it then returns
        // all known children of that block.
        //
        // zcashd: pass
        const BLOCK_RANGE: (usize, usize) = (3, 9);

        let query = Message::GetBlocks(LocatorHashes::new(
            vec![
                Hash::new([19; 32]),
                Hash::new([22; 32]),
                SEED_BLOCKS[BLOCK_RANGE.0].double_sha256().unwrap(),
            ],
            SEED_BLOCKS[BLOCK_RANGE.1].double_sha256().unwrap(),
        ));

        let result = run_test_case(query).await.unwrap();
        let expected = Message::Inv(Inv::new(
            SEED_BLOCK_HASHES[BLOCK_RANGE.0 + 1..BLOCK_RANGE.1].to_owned(),
        ));
        assert_matches!(result, QueryResult::Reply(msg) if *msg == expected);
    }

    #[tokio::test]
    async fn from_block_3_with_stop_hash_off_chain() {
        // Sends a query for all blocks starting from [3], but with
        // a stop_hash that does not match any block on the chain.
        // We expect the node to send all blocks from [3] onwards since the
        // stop_hash doesn't match any block.
        //
        // zcashd: pass
        const BLOCK_INDEX: usize = 3;

        let query = Message::GetBlocks(LocatorHashes::new(
            vec![SEED_BLOCKS[BLOCK_INDEX].double_sha256().unwrap()],
            Hash::new([22; 32]),
        ));

        let result = run_test_case(query).await.unwrap();
        let expected = Message::Inv(Inv::new(SEED_BLOCK_HASHES[BLOCK_INDEX + 1..].to_owned()));
        assert_matches!(result, QueryResult::Reply(msg) if *msg == expected);
    }
}

/// Represents the Ok(result) of [`run_test_case()`] query.
#[derive(Debug)]

enum QueryResult {
    /// Replied to the query with this [`Message`].
    Reply(Box<Message>),
    /// Ignored the query.
    Ignored,
}

/// Starts a node seeded with the initial testnet chain, connects a single
/// SyntheticNode and sends a query. The node's response to this query is
/// then returned.
async fn run_test_case(query: Message) -> io::Result<QueryResult> {
    // Spin up a node instance with knowledge of the initial testnet-chain.
    let mut node = Node::new().unwrap();
    node.initial_action(Action::SeedWithTestnetBlocks(SEED_BLOCKS.len()))
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
    synthetic_node
        .send_direct_message(node.addr(), query)
        .await?;

    // Use Ping-Pong to check node's response.
    const RECV_TIMEOUT: Duration = Duration::from_millis(100);
    let result = match synthetic_node
        .ping_pong_timeout(node.addr(), RECV_TIMEOUT)
        .await
    {
        Ok(_) => Ok(QueryResult::Ignored),
        Err(PingPongError::Unexpected(msg)) => Ok(QueryResult::Reply(msg)),
        Err(err) => Err(err.into()),
    };

    // Gracefully shut down the nodes.
    synthetic_node.shut_down();
    node.stop()?;

    result
}
