//! Contains test cases which cover ZG-CONFORMANCE-017
//!
//! The node responds to `GetHeaders` request with a list of block headers based on the provided range.
//!
//! Three broad categories are tested.
//!  1. no-range limit (stop_hash = [0]).
//!  2. stop_hash == start_hash (i.e. the range should be zero).
//!  3. ranged queries (stop_hash is valid).
//!
//! Note: Zebra does not support seeding with chain data and as such cannot run any of these tests successfully.

use std::io;

use crate::{
    protocol::{
        message::Message,
        payload::{
            block::{Headers, LocatorHashes},
            Hash,
        },
    },
    tests::conformance::query::{run_test_query, SEED_BLOCKS},
};

/// Contains a [`Message::GetHeaders`] query.
struct GetHeaders(Message);

impl GetHeaders {
    /// Creates a [`GetHeaders`] query with a single block locator hash.
    /// The hashes used will be the [`SEED_BLOCKS`] at the given indices.
    ///
    /// If `stop_hash` is [None] then the stop hash will be zeroed out.
    fn from_indices(locator_index: usize, stop_hash: Option<usize>) -> Self {
        let stop_hash =
            stop_hash.map_or_else(Hash::zeroed, |i| SEED_BLOCKS[i].double_sha256().unwrap());

        let block_locator_hashes = vec![SEED_BLOCKS[locator_index].double_sha256().unwrap()];

        Self(Message::GetHeaders(LocatorHashes::new(
            block_locator_hashes,
            stop_hash,
        )))
    }

    fn from_hashes(block_locator_hashes: Vec<Hash>, stop_hash: Hash) -> Self {
        Self(Message::GetHeaders(LocatorHashes::new(
            block_locator_hashes,
            stop_hash,
        )))
    }
}

/// The response of a node to a query.
#[derive(Debug, PartialEq)]
enum Response {
    /// Replied with [`Message::Headers(Headers::empty())`]
    EmptyHeaders,
    /// Replied to the query with this [`Message`].
    Reply(Box<Message>),
    /// Received multiple replies.
    Replies(Vec<Message>),
    /// Ignored the query.
    Ignored,
}

impl Response {
    /// Creates a [`Response::Reply`] containing [`Message::Headers`] comprised of
    /// all the [`SEED_BLOCKS`] headers in the given range.
    ///
    /// A missing end index is interpreted as `SEED_BLOCKS.len()`.
    fn headers_with_range(start: usize, end: Option<usize>) -> Self {
        let end = end.unwrap_or(SEED_BLOCKS.len());

        let headers = SEED_BLOCKS[start..end]
            .iter()
            .map(|block| block.header.clone())
            .collect();

        Self::Reply(Message::Headers(Headers::new(headers)).into())
    }
}

mod stop_hash_is_zero {
    //! No range limit tests (stop_hash = [0]).
    use super::*;

    #[tokio::test]
    async fn from_block_0_onwards() {
        // zcashd: pass
        let index = 0;
        let response = run_test_case(GetHeaders::from_indices(index, None))
            .await
            .unwrap();
        let expected = Response::headers_with_range(index + 1, None);
        assert_eq!(response, expected);
    }

    #[tokio::test]
    async fn from_block_1_onwards() {
        // zcashd: pass
        let index = 1;
        let response = run_test_case(GetHeaders::from_indices(index, None))
            .await
            .unwrap();
        let expected = Response::headers_with_range(index + 1, None);
        assert_eq!(response, expected);
    }

    #[tokio::test]
    async fn from_block_5_onwards() {
        // zcashd: pass
        let index = 5;
        let response = run_test_case(GetHeaders::from_indices(index, None))
            .await
            .unwrap();
        let expected = Response::headers_with_range(index + 1, None);
        assert_eq!(response, expected);
    }

    #[tokio::test]
    async fn from_penultimate_block_onwards() {
        // zcashd: pass
        let index = SEED_BLOCKS.len() - 2;
        let response = run_test_case(GetHeaders::from_indices(index, None))
            .await
            .unwrap();
        let expected = Response::headers_with_range(index + 1, None);
        assert_eq!(response, expected);
    }

    #[tokio::test]
    async fn final_block() {
        // We expect an empty Headers list.
        //
        // zcashd: pass
        let index = SEED_BLOCKS.len() - 1;
        let response = run_test_case(GetHeaders::from_indices(index, None))
            .await
            .unwrap();
        let expected = Response::EmptyHeaders;
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
        let query = GetHeaders::from_hashes(
            vec![
                SEED_BLOCKS[index].double_sha256().unwrap(),
                SEED_BLOCKS[index + 1].double_sha256().unwrap(),
            ],
            Hash::zeroed(),
        );

        let response = run_test_case(query).await.unwrap();
        let expected = Response::headers_with_range(index + 1, None);
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
        let query = GetHeaders::from_hashes(
            vec![
                Hash::new([19; 32]),
                Hash::new([22; 32]),
                SEED_BLOCKS[index].double_sha256().unwrap(),
            ],
            Hash::zeroed(),
        );

        let response = run_test_case(query).await.unwrap();
        let expected = Response::headers_with_range(index + 1, None);
        assert_eq!(response, expected);
    }
}

mod stop_hash_is_start_hash {
    use super::*;

    #[tokio::test]
    async fn from_block_0_to_0() {
        // zcashd: fail (sends all blocks[1+] - same behaviour as if query was not range limited)
        let index = 0;
        let response = run_test_case(GetHeaders::from_indices(index, Some(index)))
            .await
            .unwrap();
        let expected = Response::EmptyHeaders;
        assert_eq!(response, expected);
    }

    #[tokio::test]
    async fn from_block_4_to_4() {
        // zcashd: fail (sends all blocks[5+] - same behaviour as if query was not range limited)
        let index = 4;
        let response = run_test_case(GetHeaders::from_indices(index, Some(index)))
            .await
            .unwrap();
        let expected = Response::EmptyHeaders;
        assert_eq!(response, expected);
    }

    #[tokio::test]
    async fn from_final_block_to_final_block() {
        // zcashd: pass
        let index = SEED_BLOCKS.len() - 1;
        let response = run_test_case(GetHeaders::from_indices(index, Some(index)))
            .await
            .unwrap();
        let expected = Response::EmptyHeaders;
        assert_eq!(response, expected);
    }
}

mod ranged {
    use super::*;

    #[tokio::test]
    async fn from_block_0_to_1() {
        // zcashd: pass
        let range = (0, 1);
        let response = run_test_case(GetHeaders::from_indices(range.0, Some(range.1)))
            .await
            .unwrap();
        let expected = Response::headers_with_range(range.0 + 1, Some(range.1 + 1));
        assert_eq!(response, expected);
    }

    #[tokio::test]
    async fn from_block_0_to_5() {
        // zcashd: pass
        let range = (0, 5);
        let response = run_test_case(GetHeaders::from_indices(range.0, Some(range.1)))
            .await
            .unwrap();
        let expected = Response::headers_with_range(range.0 + 1, Some(range.1 + 1));
        assert_eq!(response, expected);
    }

    #[tokio::test]
    async fn from_block_0_to_final_block() {
        // zcashd: pass
        let range = (0, SEED_BLOCKS.len() - 1);
        let response = run_test_case(GetHeaders::from_indices(range.0, Some(range.1)))
            .await
            .unwrap();
        let expected = Response::headers_with_range(range.0 + 1, Some(range.1 + 1));
        assert_eq!(response, expected);
    }

    #[tokio::test]
    async fn from_block_3_to_9() {
        // zcashd: pass
        let range = (3, 9);
        let response = run_test_case(GetHeaders::from_indices(range.0, Some(range.1)))
            .await
            .unwrap();
        let expected = Response::headers_with_range(range.0 + 1, Some(range.1 + 1));
        assert_eq!(response, expected);
    }

    #[tokio::test]
    async fn from_penultimate_block_to_final_block() {
        // zcashd: pass
        let range = (SEED_BLOCKS.len() - 2, SEED_BLOCKS.len() - 1);
        let response = run_test_case(GetHeaders::from_indices(range.0, Some(range.1)))
            .await
            .unwrap();
        let expected = Response::headers_with_range(range.0 + 1, Some(range.1 + 1));
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
        let query = GetHeaders::from_hashes(
            vec![
                Hash::new([19; 32]),
                Hash::new([22; 32]),
                SEED_BLOCKS[range.0].double_sha256().unwrap(),
            ],
            SEED_BLOCKS[range.1].double_sha256().unwrap(),
        );
        let response = run_test_case(query).await.unwrap();
        let expected = Response::headers_with_range(range.0 + 1, Some(range.1 + 1));
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

        let query = GetHeaders::from_hashes(
            vec![SEED_BLOCKS[index].double_sha256().unwrap()],
            Hash::new([22; 32]),
        );

        let response = run_test_case(query).await.unwrap();
        let expected = Response::headers_with_range(index + 1, None);
        assert_eq!(response, expected);
    }
}

/// A wrapper around [`run_test_query`] which maps its output to [`Response`].
async fn run_test_case(query: GetHeaders) -> io::Result<Response> {
    let mut reply = run_test_query(query.0).await?;

    let response = match reply.len() {
        0 => Response::Ignored,
        1 if reply[0] == Message::Headers(Headers::empty()) => Response::EmptyHeaders,
        1 => Response::Reply(reply.pop().unwrap().into()),
        _ => Response::Replies(reply),
    };

    Ok(response)
}
