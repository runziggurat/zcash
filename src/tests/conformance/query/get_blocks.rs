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

use std::io;

use crate::{
    protocol::{
        message::Message,
        payload::{block::LocatorHashes, Hash, Inv},
    },
    tests::conformance::query::{run_test_query, SEED_BLOCKS},
};

/// Contains a [`Message::GetBlocks`] query.
struct GetBlocks(Message);

impl GetBlocks {
    /// Creates a [`GetBlocks`] query with a single block locator hash.
    /// The hashes used will be the [`SEED_BLOCKS`] at the given indices.
    ///
    /// If `stop_hash` is [None] then the stop hash will be zeroed out.
    fn from_indices(locator_index: usize, stop_hash: Option<usize>) -> Self {
        let stop_hash =
            stop_hash.map_or_else(Hash::zeroed, |i| SEED_BLOCKS[i].double_sha256().unwrap());

        let block_locator_hashes = vec![SEED_BLOCKS[locator_index].double_sha256().unwrap()];

        Self(Message::GetBlocks(LocatorHashes::new(
            block_locator_hashes,
            stop_hash,
        )))
    }

    fn from_hashes(block_locator_hashes: Vec<Hash>, stop_hash: Hash) -> Self {
        Self(Message::GetBlocks(LocatorHashes::new(
            block_locator_hashes,
            stop_hash,
        )))
    }
}

/// The response of a node to a query.
#[derive(Debug, PartialEq)]
enum Response {
    /// Replied to the query with this [`Message`].
    Reply(Box<Message>),
    /// Received multiple replies.
    Replies(Vec<Message>),
    /// Ignored the query.
    Ignored,
}

impl Response {
    /// Creates a [`Response::Reply`] containing [`Message::Inv`] whose inventory
    /// hashes comprises of all [`SEED_BLOCKS`] in the given range.
    ///
    /// A missing end index is interpreted as `SEED_BLOCKS.len()`.
    fn inv_with_range(start: usize, end: Option<usize>) -> Self {
        let end = end.unwrap_or_else(|| SEED_BLOCKS.len());

        let inv_hashes = SEED_BLOCKS[start..end]
            .iter()
            .map(|block| block.inv_hash())
            .collect();

        Self::Reply(Message::Inv(Inv::new(inv_hashes)).into())
    }
}

mod stop_hash_is_zero {
    //! No range limit tests (stop_hash = [0]).
    use super::*;

    #[tokio::test]
    async fn from_block_0_onwards() {
        // zcashd: pass
        let index = 0;
        let response = run_test_case(GetBlocks::from_indices(index, None))
            .await
            .unwrap();
        let expected = Response::inv_with_range(index + 1, None);
        assert_eq!(response, expected);
    }

    #[tokio::test]
    async fn from_block_1_onwards() {
        // zcashd: pass
        let index = 1;
        let response = run_test_case(GetBlocks::from_indices(index, None))
            .await
            .unwrap();
        let expected = Response::inv_with_range(index + 1, None);
        assert_eq!(response, expected);
    }

    #[tokio::test]
    async fn from_block_5_onwards() {
        // zcashd: pass
        let index = 5;
        let response = run_test_case(GetBlocks::from_indices(index, None))
            .await
            .unwrap();
        let expected = Response::inv_with_range(index + 1, None);
        assert_eq!(response, expected);
    }

    #[tokio::test]
    async fn from_penultimate_block_onwards() {
        // zcashd: pass
        let index = SEED_BLOCKS.len() - 2;
        let response = run_test_case(GetBlocks::from_indices(index, None))
            .await
            .unwrap();
        let expected = Response::inv_with_range(index + 1, None);
        assert_eq!(response, expected);
    }

    #[tokio::test]
    async fn final_block_is_ignored() {
        // Test that we get no response for the final block in the known-chain
        // (this is the behaviour exhibited by zcashd - a more well-formed response
        // might be sending an empty inventory instead).
        //
        // zcashd: pass
        let index = SEED_BLOCKS.len() - 1;
        let response = run_test_case(GetBlocks::from_indices(index, None))
            .await
            .unwrap();
        let expected = Response::Ignored;
        assert_eq!(response, expected);
    }

    #[tokio::test]
    async fn out_of_order() {
        // Node expects the block hashes in reverse order, i.e. newest first.
        // It should latch onto the first known hash and ignore the rest.
        // In this test we swop the hash order and expect it to ignore the
        // second hash.
        //
        // zcashd: pass
        let index = 7;
        let query = GetBlocks::from_hashes(
            vec![
                SEED_BLOCKS[index].double_sha256().unwrap(),
                SEED_BLOCKS[index + 1].double_sha256().unwrap(),
            ],
            Hash::zeroed(),
        );

        let response = run_test_case(query).await.unwrap();
        let expected = Response::inv_with_range(index + 1, None);
        assert_eq!(response, expected);
    }

    #[tokio::test]
    async fn off_chain_correction() {
        // Test that we get corrected if we are "off chain".
        // We expect that unknown hashes get ignored, until it finds a known hash; it then returns
        // all known children of that block.
        //
        // zcashd: pass
        let index = 1;
        let query = GetBlocks::from_hashes(
            vec![
                Hash::new([19; 32]),
                Hash::new([22; 32]),
                SEED_BLOCKS[index].double_sha256().unwrap(),
            ],
            Hash::zeroed(),
        );

        let response = run_test_case(query).await.unwrap();
        let expected = Response::inv_with_range(index + 1, None);
        assert_eq!(response, expected);
    }
}

mod stop_hash_is_start_hash {
    use super::*;

    #[tokio::test]
    async fn from_block_0_to_0() {
        // zcashd: fail (sends all blocks[1+] - same behaviour as if query was not range limited)
        let index = 0;
        let response = run_test_case(GetBlocks::from_indices(index, Some(index)))
            .await
            .unwrap();
        let expected = Response::Ignored;
        assert_eq!(response, expected);
    }

    #[tokio::test]
    async fn from_block_4_to_4() {
        // zcashd: fail (sends all blocks[5+] - same behaviour as if query was not range limited)
        let index = 4;
        let response = run_test_case(GetBlocks::from_indices(index, Some(index)))
            .await
            .unwrap();
        let expected = Response::Ignored;
        assert_eq!(response, expected);
    }

    #[tokio::test]
    async fn from_final_block_to_final_block() {
        // zcashd: pass
        let index = SEED_BLOCKS.len() - 1;
        let response = run_test_case(GetBlocks::from_indices(index, Some(index)))
            .await
            .unwrap();
        let expected = Response::Ignored;
        assert_eq!(response, expected);
    }
}

mod ranged {
    use super::*;

    #[tokio::test]
    async fn from_block_0_to_1() {
        // zcashd: pass
        let response = run_test_case(GetBlocks::from_indices(0, Some(1)))
            .await
            .unwrap();
        let expected = Response::Ignored;
        assert_eq!(response, expected);
    }

    #[tokio::test]
    async fn from_block_0_to_5() {
        // zcashd: pass
        let range = (0, 5);
        let response = run_test_case(GetBlocks::from_indices(range.0, Some(range.1)))
            .await
            .unwrap();
        let expected = Response::inv_with_range(range.0 + 1, Some(range.1));
        assert_eq!(response, expected);
    }

    #[tokio::test]
    async fn from_block_0_to_final_block() {
        // zcashd: pass
        let range = (0, SEED_BLOCKS.len() - 1);
        let response = run_test_case(GetBlocks::from_indices(range.0, Some(range.1)))
            .await
            .unwrap();
        let expected = Response::inv_with_range(range.0 + 1, Some(range.1));
        assert_eq!(response, expected);
    }

    #[tokio::test]
    async fn from_block_3_to_9() {
        // zcashd: pass
        let range = (3, 9);
        let response = run_test_case(GetBlocks::from_indices(range.0, Some(range.1)))
            .await
            .unwrap();
        let expected = Response::inv_with_range(range.0 + 1, Some(range.1));
        assert_eq!(response, expected);
    }

    #[tokio::test]
    async fn from_penultimate_block_to_final_block() {
        // zcashd: pass
        let range = (SEED_BLOCKS.len() - 2, SEED_BLOCKS.len() - 1);
        let response = run_test_case(GetBlocks::from_indices(range.0, Some(range.1)))
            .await
            .unwrap();
        let expected = Response::Ignored;
        assert_eq!(response, expected);
    }

    #[tokio::test]
    async fn off_chain_correction() {
        // Test that we get corrected if we are "off chain".
        // We expect that unknown hashes get ignored, until it finds a known hash; it then returns
        // all known children of that block.
        //
        // zcashd: pass
        let range = (3, 9);
        let query = GetBlocks::from_hashes(
            vec![
                Hash::new([19; 32]),
                Hash::new([22; 32]),
                SEED_BLOCKS[range.0].double_sha256().unwrap(),
            ],
            SEED_BLOCKS[range.1].double_sha256().unwrap(),
        );
        let response = run_test_case(query).await.unwrap();
        let expected = Response::inv_with_range(range.0 + 1, Some(range.1));
        assert_eq!(response, expected);
    }

    #[tokio::test]
    async fn stop_hash_off_chain() {
        // Sends a query for all blocks starting from [3], but with
        // a stop_hash that does not match any block on the chain.
        // We expect the node to send all blocks from [3] onwards since the
        // stop_hash doesn't match any block.
        //
        // zcashd: pass
        let index = 3;

        let query = GetBlocks::from_hashes(
            vec![SEED_BLOCKS[index].double_sha256().unwrap()],
            Hash::new([22; 32]),
        );

        let response = run_test_case(query).await.unwrap();
        let expected = Response::inv_with_range(index + 1, None);
        assert_eq!(response, expected);
    }
}

/// A wrapper around [`run_test_query`] which maps its output to [`Response`].
async fn run_test_case(query: GetBlocks) -> io::Result<Response> {
    let mut reply = run_test_query(query.0).await?;

    let response = if reply.is_empty() {
        Response::Ignored
    } else if reply.len() == 1 {
        Response::Reply(reply.pop().unwrap().into())
    } else {
        Response::Replies(reply)
    };

    Ok(response)
}
