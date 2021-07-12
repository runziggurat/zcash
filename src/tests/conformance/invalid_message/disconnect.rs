//! Contains test cases which cover ZG-CONFORMANCE-011.
//!
//! The node disconnects for trivial (non-fuzz, non-malicious) cases.
//!
//! [`Ping`](Message::Ping) timeout (not implemented)[^not_implemented].
//! [`Pong`](Message::Pong) with wrong [`Nonce`].
//! [`GetData`](Message::GetData) containing both [`Block`][inv_block] and [`Tx`][inv_tx] types in [`InvHash`][inv_hash].
//! [`Inv`](Message::Inv) containing both [`Block`][inv_block] and [`Tx`][inv_tx] types in [`InvHash`][inv_hash].
//! [`Addr`](Message::Addr) containing a [`NetworkAddr`][net_addr] *without* a  timestamp.
//!
//! [inv_block]: crate::protocol::payload::inv::ObjectKind::Block
//! [inv_tx]: crate::protocol::payload::inv::ObjectKind::Tx
//! [inv_hash]: crate::protocol::payload::inv::InvHash
//! [net_addr]: crate::protocol::payload::addr::NetworkAddr
//!
//! [^not_implemented]: This test is not implemented as ZCashd's Ping timeout is 20 minutes which is simply too long.

use std::{
    io,
    net::{IpAddr, Ipv4Addr, SocketAddr},
    time::Duration,
};

use crate::{
    protocol::{
        message::{
            constants::{ADDR_COMMAND, HEADER_LEN},
            Message, MessageHeader,
        },
        payload::{addr::NetworkAddr, block::Block, codec::Codec, Addr, Inv, Nonce},
    },
    setup::node::{Action, Node},
    tools::{
        message_filter::{Filter, MessageFilter},
        synthetic_node::{PingPongError, SyntheticNode},
    },
};

const DC_TIMEOUT: Duration = Duration::from_secs(1);

#[tokio::test]
async fn pong_with_wrong_nonce() {
    // zcashd: fail (message ignored)
    // zebra:  fail (message ignored)
    const PING_TIMEOUT: Duration = Duration::from_secs(1);

    let mut node = Node::new().unwrap();
    node.initial_action(Action::WaitForConnection)
        .start()
        .await
        .unwrap();
    // Create SyntheticNode which lets through Ping's
    let mut synthetic_node = SyntheticNode::builder()
        .with_full_handshake()
        .with_message_filter(
            MessageFilter::with_all_auto_reply().with_ping_filter(Filter::Disabled),
        )
        .build()
        .await
        .unwrap();

    synthetic_node.connect(node.addr()).await.unwrap();

    // Wait for a Ping request.
    match synthetic_node.recv_message_timeout(PING_TIMEOUT).await {
        Ok((_, Message::Ping(_))) => synthetic_node
            .send_direct_message(node.addr(), Message::Pong(Nonce::default()))
            .await
            .unwrap(),
        Ok((_, message)) => {
            panic!("Unexpected message while waiting for Ping: {}", message);
        }
        Err(err) => {
            panic!("Error waiting for Ping: {:?}", err);
        }
    }

    // Use Ping-Pong to check node's response.
    // We expect a disconnect.
    match synthetic_node
        .ping_pong_timeout(node.addr(), DC_TIMEOUT)
        .await
    {
        Err(PingPongError::ConnectionAborted) => {}
        Ok(_) => panic!("Message was ignored."),
        Err(err) => panic!("Connection was not aborted: {:?}", err),
    }

    synthetic_node.shut_down();
    node.stop().unwrap();
}

#[tokio::test]
async fn get_data_with_mixed_types() {
    // zcashd: fail (replies with Block)
    // zebra:  pass
    let genesis_block = Block::testnet_genesis();
    let mixed_inv = vec![genesis_block.inv_hash(), genesis_block.txs[0].inv_hash()];
    let message = Message::GetData(Inv::new(mixed_inv));
    run_test_case_message(message).await.unwrap();
}

#[tokio::test]
async fn inv_with_mixed_types() {
    // zcashd: fail (message ignored)
    // zebra:  pass

    // Inv with mixed inventory (using non-genesis block since all node's "should" have genesis already,
    // which makes advertising it non-sensical).
    let block_1 = Block::testnet_1();
    let mixed_inv = vec![block_1.inv_hash(), block_1.txs[0].inv_hash()];
    let message = Message::Inv(Inv::new(mixed_inv));
    run_test_case_message(message).await.unwrap();
}

#[tokio::test]
async fn addr_without_timestamp() {
    // zcashd: fail (replies with Reject(Malformed))
    // zebra:  pass

    // Create a Addr message and encode it. This encoding includes the timestamp.
    let message = Message::Addr(Addr::new(vec![NetworkAddr::new(SocketAddr::new(
        IpAddr::V4(Ipv4Addr::new(1, 2, 3, 4)),
        10,
    ))]));
    let mut payload = Vec::new();
    message.encode(&mut payload).unwrap();

    // Remove the timestamp bytes.
    //
    // First byte is a VarInt=1 containing the length of our NetworkAddr vector (1).
    // The next four bytes are the timestamp encoded as a u32.
    payload.drain(1..5);

    // Encode the full message (header + payload).
    //
    // Note that we cannot use the header from `message.encode()` as it would be generated
    // from the incorrect payload (pre-timestamp removal). Specifically the check-sum would
    // be incorrect.
    let header = MessageHeader::new(ADDR_COMMAND, &payload);
    let mut buffer = Vec::with_capacity(HEADER_LEN + payload.len());
    header.encode(&mut buffer).unwrap();
    buffer.append(&mut payload);

    run_test_case_bytes(buffer).await.unwrap();
}

async fn run_test_case_message(message: Message) -> io::Result<()> {
    let mut buffer = Vec::new();
    message.encode(&mut buffer)?;
    run_test_case_bytes(buffer).await
}

async fn run_test_case_bytes(bytes: Vec<u8>) -> io::Result<()> {
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

    synthetic_node.send_direct_bytes(node.addr(), bytes).await?;

    // Use Ping-Pong to check node's response.
    // We expect a disconnect.
    use PingPongError::*;
    let result = match synthetic_node
        .ping_pong_timeout(node.addr(), DC_TIMEOUT)
        .await
    {
        Err(ConnectionAborted) => Ok(()),
        Ok(_) => Err(io::Error::new(io::ErrorKind::Other, "Message was ignored")),
        Err(Unexpected(msg)) => Err(io::Error::new(
            io::ErrorKind::Other,
            format!("Message was replied to with {}.", msg),
        )),
        Err(Timeout(_)) => Err(io::Error::new(
            io::ErrorKind::TimedOut,
            "Timeout waiting for disconnect.",
        )),
        Err(err) => Err(io::Error::new(
            io::ErrorKind::Other,
            format!("Error waiting for disconnect: {:?}", err),
        )),
    };

    synthetic_node.shut_down();
    node.stop()?;

    result
}
