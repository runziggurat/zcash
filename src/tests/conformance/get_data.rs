use crate::{
    protocol::{
        message::Message,
        payload::{
            block::Block,
            inv::{InvHash, ObjectKind},
            Hash, Inv,
        },
    },
    setup::node::{Action, Node},
    tools::{synthetic_node::SyntheticNode, TIMEOUT},
};

use assert_matches::assert_matches;

#[tokio::test]
async fn get_data_blocks() {
    // ZG-CONFORMANCE-017, blocks portion
    //
    // The node responds to `GetData` requests with the appropriate transaction or block as requested by the peer.
    //
    // We test the following conditions:
    //  1. query for i=1..3 blocks
    //  2. a non-existing block
    //  3. a mixture of existing and non-existing blocks
    //
    // Test procedure:
    //  1. Create a node and seed it with the testnet chain
    //  2. Establish a peer node
    //  3. For each test case:
    //      a) send GetData
    //      b) receive a series Blocks
    //      c) assert Block received matches expectations
    //
    // The test currently fails for both Zebra and zcashd.
    //
    // Current behaviour:
    //
    //  zcashd: Ignores non-existing block requests, we expect `NotFound` to be sent but it never does (both in cases 2 and 3).
    //
    //  zebra: does not support seeding as yet, and therefore cannot perform this test.

    // Create a node with knowledge of the initial testnet blocks
    let mut node = Node::new().unwrap();
    node.initial_action(Action::SeedWithTestnetBlocks(11))
        .start()
        .await
        .unwrap();

    // block headers and hashes
    let blocks = vec![
        Box::new(Block::testnet_genesis()),
        Box::new(Block::testnet_1()),
        Box::new(Block::testnet_2()),
    ];

    let inv_blocks = blocks
        .iter()
        .map(|block| block.inv_hash())
        .collect::<Vec<_>>();

    // Establish a peer node
    let mut synthetic_node = SyntheticNode::builder()
        .with_full_handshake()
        .with_all_auto_reply()
        .build()
        .await
        .unwrap();

    synthetic_node.connect(node.addr()).await.unwrap();

    // Query for the first i blocks
    for i in 0..blocks.len() {
        synthetic_node
            .send_direct_message(
                node.addr(),
                Message::GetData(Inv::new(inv_blocks[..=i].to_vec())),
            )
            .await
            .unwrap();

        // Expect the i blocks
        for (j, expected_block) in blocks.iter().enumerate().take(i + 1) {
            let (_, block) = synthetic_node.recv_message_timeout(TIMEOUT).await.unwrap();
            let block = assert_matches!(block, Message::Block(block) => block);
            assert_eq!(block, *expected_block, "run {}, {}", i, j);
        }
    }

    // Query for a non-existant block
    let non_existant = InvHash::new(ObjectKind::Block, Hash::new([17; 32]));
    let non_existant_inv = Inv::new(vec![non_existant]);

    synthetic_node
        .send_direct_message(node.addr(), Message::GetData(non_existant_inv.clone()))
        .await
        .unwrap();

    let (_, not_found) = synthetic_node.recv_message_timeout(TIMEOUT).await.unwrap();
    let not_found = assert_matches!(not_found, Message::NotFound(not_found) => not_found);
    assert_eq!(not_found, non_existant_inv);

    // Query a mixture of existing and non-existing blocks
    let mut mixed_blocks = inv_blocks;
    mixed_blocks.insert(1, non_existant);
    mixed_blocks.push(non_existant);

    let expected = vec![
        Message::Block(Box::new(Block::testnet_genesis())),
        Message::NotFound(non_existant_inv.clone()),
        Message::Block(Box::new(Block::testnet_1())),
        Message::Block(Box::new(Block::testnet_2())),
        Message::NotFound(non_existant_inv),
    ];

    synthetic_node
        .send_direct_message(node.addr(), Message::GetData(Inv::new(mixed_blocks)))
        .await
        .unwrap();

    for expected_message in expected {
        let (_, message) = synthetic_node.recv_message_timeout(TIMEOUT).await.unwrap();
        assert_eq!(message, expected_message);
    }

    // Gracefully shut down the nodes.
    synthetic_node.shut_down();
    node.stop().unwrap();
}
