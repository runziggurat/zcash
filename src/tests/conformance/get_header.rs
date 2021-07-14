use crate::{
    protocol::{
        message::Message,
        payload::{
            block::{Block, LocatorHashes},
            Hash,
        },
    },
    setup::node::{Action, Node},
    tools::{synthetic_node::SyntheticNode, TIMEOUT},
};

use assert_matches::assert_matches;

#[tokio::test]
async fn correctly_lists_blocks() {
    // ZG-CONFORMANCE-016
    //
    // The node responds to `GetHeaders` request with a list of block headers based on the provided range.
    //
    // We test the following conditions:
    //  1. unlimited queries i.e. stop_hash = 0
    //  2. range queries i.e. stop_hash = i
    //  3. a forked chain (we submit a header which doesn't match the chain)
    //
    // Test procedure:
    //  1. Create a node and seed it with the testnet chain
    //  2. Establish a peer node
    //  3. For each test case:
    //      a) send GetHeaders
    //      b) receive Headers
    //      c) assert headers received match expectations
    //
    // The test currently fails for both Zebra and zcashd.
    //
    // Current behaviour:
    //
    //  zcashd: Fails for range queries where the head of the chain equals the stop hash. We expect to receive an empty set,
    //          but instead we get header [i+1] (which exceeds stop_hash).
    //
    //  zebra: does not support seeding as yet, and therefore cannot perform this test.

    // Create a node with knowledge of the initial three testnet blocks
    let mut node = Node::new().unwrap();
    node.initial_action(Action::SeedWithTestnetBlocks(3))
        .start()
        .await
        .unwrap();

    // block headers and hashes
    let expected = Block::initial_testnet_blocks()
        .iter()
        .take(3)
        .map(|block| block.header.clone())
        .collect::<Vec<_>>();
    let hashes = expected
        .iter()
        .map(|header| header.double_sha256().unwrap())
        .collect::<Vec<_>>();

    // locator hashes are stored in reverse order
    let locator = vec![
        vec![hashes[0]],
        vec![hashes[1], hashes[0]],
        vec![hashes[2], hashes[1], hashes[0]],
    ];

    // Establish a peer node.
    let mut synthetic_node = SyntheticNode::builder()
        .with_full_handshake()
        .with_all_auto_reply()
        .build()
        .await
        .unwrap();

    synthetic_node.connect(node.addr()).await.unwrap();

    // Query for all blocks from i onwards (stop_hash = [0])
    for i in 0..expected.len() {
        synthetic_node
            .send_direct_message(
                node.addr(),
                Message::GetHeaders(LocatorHashes::new(locator[i].clone(), Hash::zeroed())),
            )
            .await
            .unwrap();

        let (_, headers) = synthetic_node.recv_message_timeout(TIMEOUT).await.unwrap();
        let headers = assert_matches!(headers, Message::Headers(headers) => headers);
        assert_eq!(
            headers.headers,
            expected[(i + 1)..],
            "test for Headers([{}..])",
            i
        );
    }

    // Query for all possible valid ranges
    let ranges: Vec<(usize, usize)> = vec![(0, 0), (0, 1), (0, 2), (1, 1), (1, 2), (2, 2)];
    for (start, stop) in ranges {
        synthetic_node
            .send_direct_message(
                node.addr(),
                Message::GetHeaders(LocatorHashes::new(locator[start].clone(), hashes[stop])),
            )
            .await
            .unwrap();

        // We use start+1 because Headers should list the blocks starting *after* the
        // final location in GetHeaders, and up to (and including) the stop-hash.
        let (_, headers) = synthetic_node.recv_message_timeout(TIMEOUT).await.unwrap();
        let headers = assert_matches!(headers, Message::Headers(headers) => headers);
        assert_eq!(
            headers.headers,
            expected[start + 1..=stop],
            "test for Headers([{}..={}])",
            start + 1,
            stop
        );
    }

    // Query as if from a fork. We replace [2], and expect to be corrected
    let mut fork_locator = locator[1].clone();
    fork_locator.insert(0, Hash::new([17; 32]));

    synthetic_node
        .send_direct_message(
            node.addr(),
            Message::GetHeaders(LocatorHashes::new(fork_locator, Hash::zeroed())),
        )
        .await
        .unwrap();

    let (_, headers) = synthetic_node.recv_message_timeout(TIMEOUT).await.unwrap();
    let headers = assert_matches!(headers, Message::Headers(headers) => headers);
    assert_eq!(headers.headers, expected[2..], "test for forked Headers");

    node.stop().unwrap();
}
