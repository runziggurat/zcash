//! Contains test cases which cover ZG-CONFORMANCE-010.
//!
//! The node should ignore the following unsolicited messages:
//!
//!  Reject, NotFound, Pong, Tx, Block, Header, Addr

use std::io;

use crate::{
    protocol::{
        message::Message,
        payload::{
            block::{Block, Headers},
            Addr, Inv, Nonce,
        },
    },
    setup::node::{Action, Node},
    tools::{synthetic_node::SyntheticNode, RECV_TIMEOUT},
};

#[tokio::test]
#[allow(non_snake_case)]
async fn c010_t1_PONG() {
    // zcashd: pass
    // zebra:  pass
    run_test_case(Message::Pong(Nonce::default()))
        .await
        .unwrap();
}

#[tokio::test]
#[allow(non_snake_case)]
async fn c010_t2_HEADERS() {
    // zcashd: pass
    // zebra:  pass
    run_test_case(Message::Headers(Headers::empty()))
        .await
        .unwrap();
}

#[tokio::test]
#[allow(non_snake_case)]
async fn c010_t3_ADDR() {
    // zcashd: pass
    // zebra:  pass
    run_test_case(Message::Addr(Addr::empty())).await.unwrap();
}

#[tokio::test]
#[allow(non_snake_case)]
async fn c010_t4_BLOCK() {
    // zcashd: pass
    // zebra:  pass
    run_test_case(Message::Block(Box::new(Block::testnet_genesis())))
        .await
        .unwrap();
}

#[tokio::test]
#[allow(non_snake_case)]
async fn c010_t5_NOT_FOUND() {
    // zcashd: pass
    // zebra:  pass
    run_test_case(Message::NotFound(Inv::new(vec![
        Block::testnet_1().txs[0].inv_hash()
    ])))
    .await
    .unwrap();
}

#[tokio::test]
#[allow(non_snake_case)]
async fn c010_t6_TX() {
    // zcashd: pass
    // zebra:  pass
    run_test_case(Message::Tx(Block::testnet_2().txs[0].clone()))
        .await
        .unwrap();
}

async fn run_test_case(message: Message) -> io::Result<()> {
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

    synthetic_node.unicast(node.addr(), message)?;

    // A response to ping would indicate the previous message was ignored.
    let result = synthetic_node
        .ping_pong_timeout(node.addr(), RECV_TIMEOUT)
        .await;

    // Gracefully shut down the nodes.
    synthetic_node.shut_down().await;
    node.stop()?;

    result?;
    Ok(())
}
