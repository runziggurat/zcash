//! Contains test cases which cover ZG-CONFORMANCE-009.
//!
//! The node should reject the following messages post-handshake:
//!
//!  Version                 - Duplicate
//!  Verack                  - Duplicate
//!  Inv(mixed types)        - Invalid
//!  Inv(multiple blocks)    - Invalid
//!  Bloom filter add        - Obsolete
//!  Bloom filter load       - Obsolete
//!  Bloom filter clear      - Obsolete

use std::io;

use crate::{
    protocol::{
        message::Message,
        payload::{block::Block, reject::CCode, FilterAdd, FilterLoad, Inv, Version},
    },
    setup::node::{Action, Node},
    tools::{
        synthetic_node::{PingPongError, SyntheticNode},
        RECV_TIMEOUT,
    },
};

#[tokio::test]
async fn version_post_handshake() {
    // zcashd: pass
    // zebra:  fail (connection terminated)
    let version = Message::Version(Version::new(
        "0.0.0.0:0".parse().unwrap(),
        "0.0.0.0:0".parse().unwrap(),
    ));

    run_test_case(version, CCode::Duplicate).await.unwrap();
}

#[tokio::test]
async fn verack_post_handshake() {
    // zcashd: fail (ignored)
    // zebra:  fail (connection terminated)
    run_test_case(Message::Verack, CCode::Duplicate)
        .await
        .unwrap();
}

#[tokio::test]
async fn mixed_inventory() {
    // TODO: is this the desired behaviour, https://github.com/ZcashFoundation/zebra/issues/2107
    // might suggest it is.
    // zcashd: fail (ignored)
    // zebra:  fail (ignored)
    let genesis_block = Block::testnet_genesis();
    let mixed_inv = vec![genesis_block.inv_hash(), genesis_block.txs[0].inv_hash()];

    run_test_case(Message::Inv(Inv::new(mixed_inv)), CCode::Invalid)
        .await
        .unwrap();
}

#[tokio::test]
async fn multi_block_inventory() {
    // zcashd: fail (ignored)
    // zebra:  fail (ignored)
    let multi_block_inv = vec![
        Block::testnet_genesis().inv_hash(),
        Block::testnet_1().inv_hash(),
        Block::testnet_2().inv_hash(),
    ];

    run_test_case(Message::Inv(Inv::new(multi_block_inv)), CCode::Invalid)
        .await
        .unwrap();
}

#[tokio::test]
async fn bloom_filter_add() {
    // zcashd: fail (ccode: Malformed)
    // zebra:  fail (ignored)
    run_test_case(Message::FilterAdd(FilterAdd::default()), CCode::Obsolete)
        .await
        .unwrap();
}

#[tokio::test]
async fn bloom_filter_load() {
    // zcashd: fail (ccode: Malformed)
    // zebra:  fail (ignored)
    run_test_case(Message::FilterLoad(FilterLoad::default()), CCode::Obsolete)
        .await
        .unwrap();
}

#[tokio::test]
async fn bloom_filter_clear() {
    // zcashd: fail (ignored)
    // zebra:  fail (ignored)
    run_test_case(Message::FilterClear, CCode::Obsolete)
        .await
        .unwrap();
}

async fn run_test_case(message: Message, expected_code: CCode) -> io::Result<()> {
    // Setup a fully handshaken connection between a node and synthetic node.
    let mut node = Node::new()?;
    node.initial_action(Action::WaitForConnection)
        .start()
        .await?;
    let mut synthetic_node = SyntheticNode::builder()
        .with_full_handshake()
        .with_all_auto_reply()
        .build()
        .await?;
    synthetic_node.connect(node.addr()).await?;

    // Send the message to be rejected.
    synthetic_node.send_direct_message(node.addr(), message)?;

    // Use Ping-Pong to check the node's response to our query. We expect a Reject message.
    let result = match synthetic_node
        .ping_pong_timeout(node.addr(), RECV_TIMEOUT)
        .await
    {
        Ok(_) => Err(io::Error::new(io::ErrorKind::Other, "Message was ignored")),
        Err(PingPongError::Unexpected(msg)) => match *msg {
            Message::Reject(reject) if reject.ccode == expected_code => Ok(()),
            Message::Reject(reject) => {
                return Err(io::Error::new(
                    io::ErrorKind::Other,
                    format!(
                        "Incorrect rejection ccode: {:?} instead of {:?}",
                        reject.ccode, expected_code
                    ),
                ))
            }
            unexpected => {
                return Err(io::Error::new(
                    io::ErrorKind::Other,
                    format!("Unexpected message received: {:?}", unexpected),
                ))
            }
        },
        Err(err) => Err(err.into()),
    };

    // clean-up
    synthetic_node.shut_down().await;
    node.stop()?;

    result
}
