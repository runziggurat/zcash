//! Contains test cases which cover ZG-CONFORMANCE-018
//!
//! The node responds to `GetData` requests with the appropriate transaction or block as requested by the peer.
//!
//! Note: Zebra does not support seeding with chain data and as such cannot run any of these tests successfully.
//!
//! Note: Zcashd currently ignores requests for non-existent blocks. We expect a [`Message::NotFound`] response.

use crate::{
    protocol::{
        message::Message,
        payload::{inv::InvHash, Hash, Inv},
    },
    tests::conformance::query::{run_test_query, SEED_BLOCKS},
};

mod single_block {
    use super::*;

    #[tokio::test]
    #[allow(non_snake_case)]
    async fn c018_t1_GET_DATA_block_last() {
        // zcashd: pass
        let block = SEED_BLOCKS.last().unwrap().clone();
        let query = Message::GetData(Inv::new(vec![block.inv_hash()]));
        let expected = vec![Message::Block(block.into())];
        let response = run_test_query(query).await.unwrap();
        assert_eq!(response, expected);
    }

    #[tokio::test]
    #[allow(non_snake_case)]
    async fn c018_t2_GET_DATA_block_first() {
        // zcashd: pass
        let block = SEED_BLOCKS.first().unwrap().clone();
        let query = Message::GetData(Inv::new(vec![block.inv_hash()]));
        let expected = vec![Message::Block(block.into())];
        let response = run_test_query(query).await.unwrap();
        assert_eq!(response, expected);
    }

    #[tokio::test]
    #[allow(non_snake_case)]
    async fn c018_t3_GET_DATA_block_5() {
        // zcashd: pass
        let block = SEED_BLOCKS[5].clone();
        let query = Message::GetData(Inv::new(vec![block.inv_hash()]));
        let expected = vec![Message::Block(block.into())];
        let response = run_test_query(query).await.unwrap();
        assert_eq!(response, expected);
    }

    #[tokio::test]
    #[allow(non_snake_case)]
    async fn c018_t4_GET_DATA_non_existent() {
        // zcashd: fail (ignores non-existent block)
        let inv = Inv::new(vec![InvHash::Block(Hash::new([17; 32]))]);
        let query = Message::GetData(inv.clone());
        let expected = vec![Message::NotFound(inv)];
        let response = run_test_query(query).await.unwrap();
        assert_eq!(response, expected);
    }
}

mod multiple_blocks {
    use super::*;

    #[tokio::test]
    #[allow(non_snake_case)]
    async fn c018_t5_GET_DATA_all() {
        // zcashd: pass
        let blocks = &SEED_BLOCKS;
        let inv_hash = blocks.iter().map(|block| block.inv_hash()).collect();
        let query = Message::GetData(Inv::new(inv_hash));
        let expected = blocks
            .iter()
            .map(|block| Message::Block(Box::new(block.clone())))
            .collect::<Vec<_>>();
        let response = run_test_query(query).await.unwrap();
        assert_eq!(response, expected);
    }

    #[tokio::test]
    #[allow(non_snake_case)]
    async fn c018_t6_GET_DATA_out_of_order() {
        // zcashd: pass
        let blocks = vec![&SEED_BLOCKS[3], &SEED_BLOCKS[1], &SEED_BLOCKS[7]];
        let inv_hash = blocks.iter().map(|block| block.inv_hash()).collect();
        let query = Message::GetData(Inv::new(inv_hash));
        let expected = blocks
            .iter()
            .map(|&block| Message::Block(Box::new(block.clone())))
            .collect::<Vec<_>>();
        let response = run_test_query(query).await.unwrap();
        assert_eq!(response, expected);
    }

    #[tokio::test]
    #[allow(non_snake_case)]
    async fn c018_t7_GET_DATA_blocks_1_to_8() {
        // zcashd: pass
        let blocks = &SEED_BLOCKS[1..=8];
        let inv_hash = blocks.iter().map(|block| block.inv_hash()).collect();
        let query = Message::GetData(Inv::new(inv_hash));
        let expected = blocks
            .iter()
            .map(|block| Message::Block(Box::new(block.clone())))
            .collect::<Vec<_>>();
        let response = run_test_query(query).await.unwrap();
        assert_eq!(response, expected);
    }

    #[tokio::test]
    #[allow(non_snake_case)]
    async fn c018_t8_GET_DATA_non_existent_blocks() {
        // zcashd: fails (ignores non-existent blocks).
        let inv = Inv::new(vec![
            InvHash::Block(Hash::new([17; 32])),
            InvHash::Block(Hash::new([211; 32])),
            InvHash::Block(Hash::new([74; 32])),
        ]);

        let query = Message::GetData(inv.clone());
        let expected = vec![Message::NotFound(inv)];
        let response = run_test_query(query).await.unwrap();
        assert_eq!(response, expected);
    }

    #[tokio::test]
    #[allow(non_snake_case)]
    async fn c018_t9_GET_DATA_mixed_existent_and_non_existent_blocks() {
        // Test a mixture of existent and non-existent blocks
        // interwoven together.
        //
        // We expect the response to contain a Block for each
        // existing request, and a single NotFound containing
        // hashes for all non-existent blocks. The order of
        // these is undefined, but since zcashd currently
        // does not send NotFound at all, it does not matter.
        //
        // zcashd: fails (ignores non-existent blocks).

        let non_existent_inv = vec![
            InvHash::Block(Hash::new([17; 32])),
            InvHash::Block(Hash::new([211; 32])),
            InvHash::Block(Hash::new([74; 32])),
        ];
        let blocks = SEED_BLOCKS
            .iter()
            .skip(3)
            .take(non_existent_inv.len())
            .collect::<Vec<_>>();

        let expected_blocks = blocks
            .iter()
            .map(|&block| Message::Block(Box::new(block.clone())))
            .collect::<Vec<_>>();
        let expected_non_existent = Message::NotFound(Inv::new(non_existent_inv.clone()));

        let mixed_inv = blocks
            .iter()
            .map(|block| block.inv_hash())
            .zip(non_existent_inv)
            .flat_map(|(a, b)| [a, b])
            .collect();

        let query = Message::GetData(Inv::new(mixed_inv));
        let response = run_test_query(query).await.unwrap();

        // Should contain expected_blocks[..] and expected_non_existent. Not sure
        // what order we should expect (if any). But since the node currently ignores
        // non-existent block queries we are free to assume any order.
        let mut expected = expected_blocks;
        expected.push(expected_non_existent);
        assert_eq!(response, expected);
    }
}
