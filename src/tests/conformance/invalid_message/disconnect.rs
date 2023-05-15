//! Contains test cases which cover ZG-CONFORMANCE-012.
//!
//! The node disconnects for trivial (non-fuzz, non-malicious) cases.
//!
//! [`Ping`](Message::Ping) timeout (not implemented)[^not_implemented].
//! [`Pong`](Message::Pong) with wrong [`Nonce`].
//! [`GetData`](Message::GetData) containing both [`Block`][inv_block] and [`Tx`][inv_tx] types in [`InvHash`][inv_hash].
//! [`Inv`](Message::Inv) containing both [`Block`][inv_block] and [`Tx`][inv_tx] types in [`InvHash`][inv_hash].
//! [`Addr`](Message::Addr) containing a [`NetworkAddr`][net_addr] *without* a  timestamp.
//!
//! [inv_block]: crate::protocol::payload::inv::InvHash::Block
//! [inv_tx]: crate::protocol::payload::inv::InvHash::Tx
//! [inv_hash]: crate::protocol::payload::inv::InvHash
//! [net_addr]: crate::protocol::payload::addr::NetworkAddr
//!
//! [^not_implemented]: This test is not implemented as ZCashd's Ping timeout is 20 minutes which is simply too long.

use std::{
    io,
    net::{IpAddr, Ipv4Addr, SocketAddr},
};

use crate::{
    protocol::{
        message::{
            constants::{ADDR_COMMAND, HEADER_LEN},
            Message, MessageHeader,
        },
        payload::{addr::NetworkAddr, block::Block, codec::Codec, Addr, Inv, Nonce, VarInt},
    },
    setup::node::{Action, Node},
    tools::{
        message_filter::{Filter, MessageFilter},
        synthetic_node::{PingPongError, SyntheticNode},
        LONG_TIMEOUT, RECV_TIMEOUT,
    },
};

#[tokio::test]
#[allow(non_snake_case)]
async fn c012_t1_PONG_with_wrong_nonce() {
    // zcashd: fail (message ignored)
    // zebra:  fail (message ignored)

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
    match synthetic_node.recv_message_timeout(RECV_TIMEOUT).await {
        Ok((_, Message::Ping(_))) => synthetic_node
            .unicast(node.addr(), Message::Pong(Nonce::default()))
            .unwrap(),
        Ok((_, message)) => {
            panic!("Unexpected message while waiting for Ping: {message}");
        }
        Err(err) => {
            panic!("Error waiting for Ping: {err:?}");
        }
    }

    // Use Ping-Pong to check node's response.
    // We expect a disconnect.
    match synthetic_node
        .ping_pong_timeout(node.addr(), LONG_TIMEOUT)
        .await
    {
        Err(PingPongError::ConnectionAborted) => {}
        Ok(_) => panic!("Message was ignored."),
        Err(err) => panic!("Connection was not aborted: {err:?}"),
    }

    synthetic_node.shut_down().await;
    node.stop().unwrap();
}

#[tokio::test]
#[allow(non_snake_case)]
async fn c012_t2_GET_DATA_with_mixed_types() {
    // zcashd: fail (replies with Block)
    // zebra:  fail (replies with NotFound)
    let genesis_block = Block::testnet_genesis();
    let mixed_inv = vec![genesis_block.inv_hash(), genesis_block.txs[0].inv_hash()];
    let message = Message::GetData(Inv::new(mixed_inv));
    run_test_case_message(message).await.unwrap();
}

#[tokio::test]
#[allow(non_snake_case)]
async fn c012_t3_INV_with_mixed_types() {
    // zcashd: fail (message ignored)
    // zebra:  fail (message ignored)

    // Inv with mixed inventory (using non-genesis block since all node's "should" have genesis already,
    // which makes advertising it non-sensical).
    let block_1 = Block::testnet_1();
    let mixed_inv = vec![block_1.inv_hash(), block_1.txs[0].inv_hash()];
    let message = Message::Inv(Inv::new(mixed_inv));
    run_test_case_message(message).await.unwrap();
}

#[tokio::test]
#[allow(non_snake_case)]
async fn c012_t4_ADDR_without_timestamp() {
    // zcashd: fail (replies with Reject(Malformed))
    // zebra:  pass

    // The NetworkAddrs we wish to send.
    let net_addrs = vec![NetworkAddr::new(SocketAddr::new(
        IpAddr::V4(Ipv4Addr::new(1, 2, 3, 4)),
        10,
    ))];

    // Timestamp byte offset in the encoded payload.
    //
    // The timestamp is the second encoded field. It follows
    // a VarInt field which indicates the number of NetWorkAddrs in
    // the message.
    let timestamp_offset = {
        let varint = VarInt::new(net_addrs.len());
        let mut varint_buffer = Vec::new();
        varint.encode(&mut varint_buffer).unwrap();
        varint_buffer.len()
    };

    // Create a Addr message and encode it. This encoding includes the timestamp.
    let message = Message::Addr(Addr::new(net_addrs));
    let mut payload = Default::default();
    message.encode(&mut payload).unwrap();
    let mut payload = payload.to_vec();

    // Remove the timestamp bytes. The length of the timestamp field is four 4 bytes (u32).p
    payload.drain(timestamp_offset..timestamp_offset + 4);

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
    let mut buffer = Default::default();
    message.encode(&mut buffer)?;
    run_test_case_bytes(buffer.to_vec()).await
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

    synthetic_node.send_direct_bytes(node.addr(), bytes)?;

    // Use Ping-Pong to check node's response.
    // We expect a disconnect.
    use PingPongError::*;
    let result = match synthetic_node
        .ping_pong_timeout(node.addr(), LONG_TIMEOUT)
        .await
    {
        Err(ConnectionAborted) => Ok(()),
        Ok(_) => Err(io::Error::new(io::ErrorKind::Other, "Message was ignored")),
        Err(Unexpected(msg)) => Err(io::Error::new(
            io::ErrorKind::Other,
            format!("Message was replied to with {msg}."),
        )),
        Err(Timeout(_)) => Err(io::Error::new(
            io::ErrorKind::TimedOut,
            "Timeout waiting for disconnect.",
        )),
        Err(err) => Err(io::Error::new(
            io::ErrorKind::Other,
            format!("Error waiting for disconnect: {err:?}"),
        )),
    };

    synthetic_node.shut_down().await;
    node.stop()?;

    result
}
