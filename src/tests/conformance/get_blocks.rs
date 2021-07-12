use crate::{
    protocol::{
        message::Message,
        payload::{
            block::{Block, LocatorHashes},
            Hash, Inv,
        },
    },
    setup::node::{Action, Node},
    tools::{synthetic_node::SyntheticNode, TIMEOUT},
};

use assert_matches::assert_matches;

#[tokio::test]
async fn get_blocks() {
    // ZG-CONFORMANCE-015
    //
    // The node responds to `GetBlocks` requests with a list of blocks based on the provided range.
    //
    // We test the following conditions:
    //  1. unlimited queries i.e. stop_hash = 0
    //  2. range queries i.e. stop_hash = i
    //  3. a forked chain (we submit a valid hash, followed by incorrect hashes)
    //
    // Test procedure:
    //  1. Create a node and seed it with the testnet chain
    //  2. Establish a peer node
    //  3. For each test case:
    //      a) send GetBlocks
    //      b) receive Inv
    //      c) assert Inv received matches expectations
    //
    // The test currently fails for both Zebra and zcashd.
    //
    // Current behaviour:
    //
    //  zcashd: Passes
    //
    //  zebra: does not support seeding as yet, and therefore cannot perform this test.
    //
    // Note: zcashd excludes the `stop_hash` from the range, whereas the spec states that it should be inclusive.
    //       We are taking current behaviour as correct.
    //
    // Note: zcashd ignores requests for the final block in the chain

    // Create a node with knowledge of the initial testnet blocks
    let mut node = Node::new().unwrap();
    node.initial_action(Action::SeedWithTestnetBlocks(11))
        .start()
        .await
        .unwrap();

    let blocks = Block::initial_testnet_blocks();

    let mut synthetic_node = SyntheticNode::builder()
        .with_full_handshake()
        .with_all_auto_reply()
        .build()
        .await
        .unwrap();

    synthetic_node.connect(node.addr()).await.unwrap();

    // Test unlimited range queries, where given the hash for block i we expect all
    // of its children as a reply. This does not apply for the last block in the chain,
    // so we skip it.
    //
    // i.e. Test that GetBlocks(i) -> Inv(i+1..)
    for (i, block) in blocks.iter().enumerate().take(blocks.len() - 1) {
        synthetic_node
            .send_direct_message(
                node.addr(),
                Message::GetBlocks(LocatorHashes::new(
                    vec![block.double_sha256().unwrap()],
                    Hash::zeroed(),
                )),
            )
            .await
            .unwrap();

        let (_, inv) = synthetic_node.recv_message_timeout(TIMEOUT).await.unwrap();
        let inv = assert_matches!(inv, Message::Inv(inv) => inv);

        // Collect inventory hashes for all blocks after i (i's children) and check the payload
        // matches.
        let inv_hashes = blocks.iter().skip(i + 1).map(|b| b.inv_hash()).collect();
        let expected = Inv::new(inv_hashes);
        assert_eq!(inv, expected);
    }

    // Test that we get no response for the final block in the known-chain
    // (this is the behaviour exhibited by zcashd - a more well-formed response
    // might be sending an empty inventory instead).
    synthetic_node
        .send_direct_message(
            node.addr(),
            Message::GetBlocks(LocatorHashes::new(
                vec![blocks.last().unwrap().double_sha256().unwrap()],
                Hash::zeroed(),
            )),
        )
        .await
        .unwrap();

    // Test message is ignored by sending Ping and receiving Pong.
    synthetic_node
        .ping_pong_timeout(node.addr(), TIMEOUT)
        .await
        .unwrap();

    // Test `hash_stop` (it should be included in the range, but zcashd excludes it -- see note).
    synthetic_node
        .send_direct_message(
            node.addr(),
            Message::GetBlocks(LocatorHashes::new(
                vec![blocks[0].double_sha256().unwrap()],
                blocks[2].double_sha256().unwrap(),
            )),
        )
        .await
        .unwrap();

    let (_, inv) = synthetic_node.recv_message_timeout(TIMEOUT).await.unwrap();
    let inv = assert_matches!(inv, Message::Inv(inv) => inv);

    // Check the payload matches.
    let expected = Inv::new(vec![blocks[1].inv_hash()]);
    assert_eq!(inv, expected);

    // Test that we get corrected if we are "off chain".
    // We expect that unknown hashes get ignored, until it finds a known hash; it then returns
    // all known children of that block.
    let locators = LocatorHashes::new(
        vec![
            blocks[1].double_sha256().unwrap(),
            Hash::new([19; 32]),
            Hash::new([22; 32]),
        ],
        Hash::zeroed(),
    );

    synthetic_node
        .send_direct_message(node.addr(), Message::GetBlocks(locators))
        .await
        .unwrap();

    let (_, inv) = synthetic_node.recv_message_timeout(TIMEOUT).await.unwrap();
    let inv = assert_matches!(inv, Message::Inv(inv) => inv);

    // Check the payload matches.
    let inv_hashes = blocks[2..].iter().map(|block| block.inv_hash()).collect();
    let expected = Inv::new(inv_hashes);
    assert_eq!(inv, expected);

    synthetic_node.shut_down();
    node.stop().unwrap();
}
