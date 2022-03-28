//! A lightweight node implementation to be used as peers in tests.

use crate::{
    protocol::{
        message::{Message, MessageHeader},
        payload::{codec::Codec, Nonce, Version},
    },
    tools::message_filter::{Filter, MessageFilter},
};

use assert_matches::assert_matches;
use bytes::{BufMut, BytesMut};
use futures_util::{sink::SinkExt, TryStreamExt};
use pea2pea::{
    connections::ConnectionSide,
    protocols::{Handshake, Reading, Writing},
    Config as NodeConfig, Connection, KnownPeers, Node, Pea2Pea,
};
use tokio::{
    sync::mpsc::{self, Receiver, Sender},
    time::timeout,
};
use tokio_util::codec::{Decoder, Encoder, Framed, LengthDelimitedCodec};
use tracing::*;

use std::{
    io::{self, Error, ErrorKind},
    net::{IpAddr, Ipv4Addr, SocketAddr},
    time::Duration,
};

/// An [`Error`](std::error::Error) type for [`SyntheticNode::ping_pong_timeout`]
pub enum PingPongError {
    /// The connection was aborted during the [`Ping`](Message::Ping)-[`Pong`](Message::Pong) exchange.
    ConnectionAborted,
    /// An [io::Error] occurred during the [`Ping`](Message::Ping)-[`Pong`](Message::Pong) exchange.
    IoErr(io::Error),
    /// Timeout was exceeded before a [`Pong`](Message::Pong) was received.
    Timeout(Duration),
    /// A message was received which was not [`Pong`](Message::Pong), or the [Pong's nonce](Nonce) did not match.
    Unexpected(Box<Message>),
}

impl std::fmt::Debug for PingPongError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        let str = match self {
            PingPongError::ConnectionAborted => "Connection aborted".to_string(),
            PingPongError::IoErr(err) => format!("{:?}", err),
            PingPongError::Timeout(duration) => {
                format!("Timeout after {0:.3}s", duration.as_secs_f32())
            }
            PingPongError::Unexpected(msg) => match &**msg {
                Message::Pong(_) => "Pong nonce did not match".to_string(),
                non_pong => format!("Expected a matching Pong, but got {:?}", non_pong),
            },
        };

        f.write_str(&str)
    }
}

impl std::fmt::Display for PingPongError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        // FIXME: Consider specialising the longer debug strings, e.g.
        //        IoErr and Unexpected.
        f.write_str(&format!("{:?}", self))
    }
}

impl std::error::Error for PingPongError {}

impl From<PingPongError> for io::Error {
    fn from(original: PingPongError) -> Self {
        use PingPongError::*;
        match original {
            ConnectionAborted => Error::new(ErrorKind::ConnectionAborted, "Connection aborted"),
            IoErr(err) => err,
            Timeout(duration) => Error::new(
                ErrorKind::TimedOut,
                format!("Timeout after {0:.3}s", duration.as_secs_f64()),
            ),
            Unexpected(msg) => Error::new(
                ErrorKind::Other,
                format!("Expected Pong, received {:?}", msg),
            ),
        }
    }
}

/// Enables tracing for all [`SyntheticNode`] instances (usually scoped by test).
pub fn enable_tracing() {
    use tracing_subscriber::{fmt, EnvFilter};

    fmt()
        .with_test_writer()
        .with_env_filter(EnvFilter::from_default_env())
        .init();
}

/// Describes the handshake to be performed by a [`SyntheticNode`].
#[derive(Debug, Clone, Copy, PartialEq)]
pub enum HandshakeKind {
    /// [`Version`] and [`Verack`] in both directions.
    ///
    /// [`Version`]: enum@crate::protocol::message::Message::Version
    /// [`Verack`]: enum@crate::protocol::message::Message::Verack
    Full,
    /// Only [`Version`] messages are exchanged.
    ///
    /// [`Version`]: enum@crate::protocol::message::Message::Version
    VersionOnly,
}

/// A builder for [`SyntheticNode`].
#[derive(Debug, Clone)]
pub struct SyntheticNodeBuilder {
    network_config: Option<NodeConfig>,
    handshake: Option<HandshakeKind>,
    message_filter: MessageFilter,
}

impl Default for SyntheticNodeBuilder {
    fn default() -> Self {
        Self {
            network_config: Some(NodeConfig {
                // Set localhost as the default IP.
                listener_ip: Some(IpAddr::V4(Ipv4Addr::LOCALHOST)),
                ..Default::default()
            }),
            handshake: None,
            message_filter: MessageFilter::with_all_disabled(),
        }
    }
}

impl SyntheticNodeBuilder {
    /// Creates a [`SyntheticNode`] with the current configuration
    pub async fn build(&self) -> io::Result<SyntheticNode> {
        // Create the pea2pea node from the config.
        let node = Node::new(self.network_config.clone()).await?;

        // Inbound channel size of 100 messages.
        let (tx, rx) = mpsc::channel(100);
        let inner_node =
            InnerNode::new(node, tx, self.message_filter.clone(), self.handshake).await;

        // Enable the read and write protocols
        inner_node.enable_reading().await;
        inner_node.enable_writing().await;

        Ok(SyntheticNode {
            inner_node,
            inbound_rx: rx,
        })
    }

    /// Creates `n` [`SyntheticNode`]'s with the current configuration, and also returns their listening address.
    pub async fn build_n(&self, n: usize) -> io::Result<(Vec<SyntheticNode>, Vec<SocketAddr>)> {
        let mut nodes = Vec::with_capacity(n);
        let mut addrs = Vec::with_capacity(n);
        for _ in 0..n {
            let node = self.build().await?;
            addrs.push(node.listening_addr());
            nodes.push(node);
        }

        Ok((nodes, addrs))
    }

    /// Sets the node's [`MessageFilter`] to [`Filter::AutoReply`].
    pub fn with_all_auto_reply(mut self) -> Self {
        self.message_filter = MessageFilter::with_all_auto_reply();
        self
    }

    /// Enables handshaking with [`HandshakeKind::Full`].
    pub fn with_full_handshake(mut self) -> Self {
        self.handshake = Some(HandshakeKind::Full);
        self
    }

    /// Enables handshaking with [`HandshakeKind::VersionOnly`].
    pub fn with_version_exchange_handshake(mut self) -> Self {
        self.handshake = Some(HandshakeKind::VersionOnly);
        self
    }

    /// Sets the node's [`MessageFilter`].
    pub fn with_message_filter(mut self, filter: MessageFilter) -> Self {
        self.message_filter = filter;
        self
    }
}

/// Convenient abstraction over a `pea2pea` node.
pub struct SyntheticNode {
    inner_node: InnerNode,
    inbound_rx: Receiver<(SocketAddr, Message)>,
}

impl SyntheticNode {
    // FIXME: remove in favour of calling `SyntheticNodeBuilder::default()` or `new` directly?
    pub fn builder() -> SyntheticNodeBuilder {
        SyntheticNodeBuilder::default()
    }

    /// Returns the listening address of the node.
    pub fn listening_addr(&self) -> SocketAddr {
        self.inner_node.node().listening_addr().unwrap()
    }

    /// Connects to the target address.
    ///
    /// If the handshake protocol is enabled it will be executed as well.
    pub async fn connect(&self, target: SocketAddr) -> io::Result<()> {
        self.inner_node.node().connect(target).await?;

        Ok(())
    }

    /// Indicates if the `addr` is registered as a connected peer.
    pub fn is_connected(&self, addr: SocketAddr) -> bool {
        self.inner_node.node().is_connected(addr)
    }

    /// Returns the number of connected peers.
    pub fn num_connected(&self) -> usize {
        self.inner_node.node().num_connected()
    }

    /// Returns a reference to the node's known peers.
    pub fn known_peers(&self) -> &KnownPeers {
        self.inner_node.node().known_peers()
    }

    /// Returns the list of active connections for this node. Should be preferred over [`known_peers`] when querying active connections.
    pub fn connected_peers(&self) -> Vec<SocketAddr> {
        self.inner_node.node.connected_addrs()
    }

    /// Waits until the node has at least one connection, and
    /// returns its SocketAddr
    pub async fn wait_for_connection(&self) -> SocketAddr {
        const SLEEP: Duration = Duration::from_millis(10);
        loop {
            // Mutating the collection is alright since this is a copy of the connections and not the actually list.
            if let Some(addr) = self.connected_peers().pop() {
                return addr;
            }

            tokio::time::sleep(SLEEP).await;
        }
    }

    /// Sends a direct message to the target address.
    pub fn send_direct_message(&self, target: SocketAddr, message: Message) -> io::Result<()> {
        self.inner_node
            .send_direct_message(target, MessageOrBytes::Message(message.into()))?;

        Ok(())
    }

    /// Sends bytes directly to the target address.
    pub fn send_direct_bytes(&self, target: SocketAddr, data: Vec<u8>) -> io::Result<()> {
        self.inner_node
            .send_direct_message(target, MessageOrBytes::Bytes(data))?;

        Ok(())
    }

    /// Reads a message from the inbound (internal) queue of the node.
    ///
    /// Messages are sent to the queue when unfiltered by the message filter.
    pub async fn recv_message(&mut self) -> (SocketAddr, Message) {
        match self.inbound_rx.recv().await {
            Some(message) => message,
            None => panic!("all senders dropped!"),
        }
    }

    // Attempts to read a message from the inbound (internal) queue of the node before the timeout
    // duration has elapsed (seconds).
    // FIXME: logging?
    pub async fn recv_message_timeout(
        &mut self,
        duration: Duration,
    ) -> io::Result<(SocketAddr, Message)> {
        match timeout(duration, self.recv_message()).await {
            Ok(message) => Ok(message),
            Err(_e) => Err(Error::new(
                ErrorKind::TimedOut,
                format!(
                    "could not read message after {0:.3}s",
                    duration.as_secs_f64()
                ),
            )),
        }
    }

    /// Sends [`Ping`], and expects [`Pong`] with a matching [`Nonce`] in reply.
    ///
    /// Uses polling to check that connection is still alive. Returns a [`PingPongError`] if:
    /// - a non-[`Pong`] message is received
    /// - a [`Pong`] with a non-matching [`Nonce`] is receives
    /// - the timeout expires
    /// - the connection breaks
    /// - an [`io::Error`] occurs
    ///
    /// Is useful for checking a node's response to a prior query.
    /// - if it was ignored, this call will succeed with `Ok(())`
    /// - if there was a reply, it will be contained in [`Unexpected`](PingPongError::Unexpected)
    /// - and [`ConnectionAborted`](PingPongError::ConnectionAborted) if the connection was terminated -
    ///
    /// [`Ping`]: enum@crate::protocol::message::Message::Ping
    /// [`Pong`]: enum@crate::protocol::message::Message::Pong
    /// [`Nonce`]: struct@crate::protocol::payload::Nonce
    pub async fn ping_pong_timeout(
        &mut self,
        target: SocketAddr,
        duration: Duration,
    ) -> Result<(), PingPongError> {
        const SLEEP: Duration = Duration::from_millis(10);

        let now = std::time::Instant::now();
        let ping_nonce = Nonce::default();
        if let Err(err) = self.send_direct_message(target, Message::Ping(ping_nonce)) {
            if !self.is_connected(target) {
                return Err(PingPongError::ConnectionAborted);
            } else {
                return Err(PingPongError::IoErr(err));
            }
        }

        while now.elapsed() < duration {
            match self.recv_message_timeout(SLEEP).await {
                Err(_timeout) => {
                    // Check that connection is still alive, so that we can exit sooner
                    if !self.is_connected(target) {
                        return Err(PingPongError::ConnectionAborted);
                    }
                }
                Ok((_, Message::Pong(nonce))) if nonce == ping_nonce => {
                    return Ok(());
                }
                Ok((_, message)) => {
                    return Err(PingPongError::Unexpected(message.into()));
                }
            }
        }

        Err(PingPongError::Timeout(duration))
    }

    /// Waits for the target to disconnect by sending a [`Ping`] request. Errors if
    /// the target responds or doesn't disconnect within the timeout.
    ///
    /// [`Ping`]: enum@crate::protocol::message::Message::Ping
    pub async fn wait_for_disconnect(
        &mut self,
        target: SocketAddr,
        duration: Duration,
    ) -> io::Result<()> {
        match self.ping_pong_timeout(target, duration).await {
            Ok(_) => Err(Error::new(ErrorKind::Other, "connection still active")),
            Err(PingPongError::ConnectionAborted) => Ok(()),
            Err(err) => Err(err.into()),
        }
    }

    /// Gracefully shuts down the node.
    pub async fn shut_down(&self) {
        self.inner_node.node().shut_down().await
    }
}

#[derive(Clone)]
struct InnerNode {
    node: Node,
    handshake: Option<HandshakeKind>,
    inbound_tx: Sender<(SocketAddr, Message)>,
    message_filter: MessageFilter,
}

impl InnerNode {
    async fn new(
        node: Node,
        tx: Sender<(SocketAddr, Message)>,
        message_filter: MessageFilter,
        handshake: Option<HandshakeKind>,
    ) -> Self {
        let node = Self {
            node,
            inbound_tx: tx,
            message_filter,
            handshake,
        };

        if handshake.is_some() {
            node.enable_handshake().await;
        }

        node
    }
}

impl Pea2Pea for InnerNode {
    fn node(&self) -> &Node {
        &self.node
    }
}

// TODO: move to protocol
struct MessageCodec {
    codec: LengthDelimitedCodec,
}

impl Default for MessageCodec {
    fn default() -> Self {
        Self {
            codec: LengthDelimitedCodec::builder()
                .length_adjustment(24)
                .length_field_offset(16)
                .little_endian()
                .num_skip(0)
                .max_frame_length(65536) // FIXME
                .new_codec(),
        }
    }
}

impl Decoder for MessageCodec {
    type Item = Message;
    type Error = io::Error;

    fn decode(&mut self, src: &mut BytesMut) -> Result<Option<Self::Item>, Self::Error> {
        let mut bytes = if let Some(bytes) = self.codec.decode(src)? {
            bytes
        } else {
            return Ok(None);
        };

        let header = MessageHeader::decode(&mut bytes)?;
        let message = Message::decode(header.command, &mut bytes)?;

        Ok(Some(message))
    }
}

impl Encoder<Vec<u8>> for MessageCodec {
    type Error = io::Error;

    fn encode(&mut self, message: Vec<u8>, dst: &mut BytesMut) -> Result<(), Self::Error> {
        dst.put_slice(&message);

        Ok(())
    }
}

impl Encoder<Message> for MessageCodec {
    type Error = io::Error;

    fn encode(&mut self, message: Message, dst: &mut BytesMut) -> Result<(), Self::Error> {
        message.encode(dst)
    }
}

impl Encoder<MessageOrBytes> for MessageCodec {
    type Error = io::Error;

    fn encode(&mut self, message: MessageOrBytes, dst: &mut BytesMut) -> Result<(), Self::Error> {
        match message {
            MessageOrBytes::Bytes(message) => Encoder::<Vec<u8>>::encode(self, message, dst),
            MessageOrBytes::Message(message) => Encoder::<Message>::encode(self, *message, dst),
        }
    }
}

// TODO: move to protocol
enum MessageOrBytes {
    Message(Box<Message>),
    Bytes(Vec<u8>),
}

#[async_trait::async_trait]
impl Reading for InnerNode {
    type Message = Message;
    type Codec = MessageCodec;

    fn codec(&self, _addr: SocketAddr) -> Self::Codec {
        Default::default()
    }

    async fn process_message(&self, source: SocketAddr, message: Self::Message) -> io::Result<()> {
        let span = self.node().span().clone();

        debug!(parent: span.clone(), "processing {:?}", message);
        match self.message_filter.message_filter_type(&message) {
            Filter::AutoReply => {
                // Autoreply with the appropriate response.
                let response = self.message_filter.reply_message(&message);

                debug!(parent: span, "auto replying with {:?}", response);
                self.send_direct_message(source, MessageOrBytes::Message(response.into()))?;
            }

            Filter::Disabled => {
                // Send the message to the node's inbound queue.
                debug!(
                    parent: span,
                    "sending the message to the node's inbound queue"
                );
                self.inbound_tx
                    .send((source, message))
                    .await
                    .expect("receiver dropped!");
            }

            Filter::Enabled => {
                // Ignore the message.
                debug!(parent: span, "message was ignored by the filter");
            }
        }

        Ok(())
    }
}

impl Writing for InnerNode {
    type Message = MessageOrBytes;
    type Codec = MessageCodec;

    fn codec(&self, _addr: SocketAddr) -> Self::Codec {
        Default::default()
    }
}

#[async_trait::async_trait]
impl Handshake for InnerNode {
    async fn perform_handshake(&self, mut conn: Connection) -> io::Result<Connection> {
        let node_conn_side = !conn.side();
        let conn_addr = conn.addr();
        let own_listening_addr = self.node().listening_addr().unwrap();
        let mut framed_stream = Framed::new(self.borrow_stream(&mut conn), MessageCodec::default());

        match (self.handshake, node_conn_side) {
            (Some(HandshakeKind::Full), ConnectionSide::Initiator) => {
                // Send and receive Version.
                let own_version = Message::Version(Version::new(conn_addr, own_listening_addr));
                framed_stream.send(own_version).await?;

                let peer_version = framed_stream.try_next().await?;
                assert_matches!(peer_version, Some(Message::Version(..)));

                // Send and receive Verack.
                framed_stream.send(Message::Verack).await?;

                let peer_verack = framed_stream.try_next().await?;
                assert_matches!(peer_verack, Some(Message::Verack));
            }
            (Some(HandshakeKind::Full), ConnectionSide::Responder) => {
                // Receive and send Version.
                let peer_version = framed_stream.try_next().await?;
                let node_addr = match peer_version {
                    Some(Message::Version(version)) => version.addr_from.addr,
                    Some(other) => {
                        let span = self.node().span().clone();
                        error!(
                            parent: span,
                            "received non-version message during handshake: {:?}", other
                        );
                        panic!("Expected Version, got {:?}", other);
                    }
                    None => return Err(io::ErrorKind::InvalidData.into()),
                };

                let own_version = Message::Version(Version::new(node_addr, own_listening_addr));
                framed_stream.send(own_version).await?;

                // Receive and send Verack.
                let peer_verack = framed_stream.try_next().await?;
                assert_matches!(peer_verack, Some(Message::Verack));

                framed_stream.send(Message::Verack).await?;
            }
            (Some(HandshakeKind::VersionOnly), ConnectionSide::Initiator) => {
                let own_version = Message::Version(Version::new(conn_addr, own_listening_addr));
                framed_stream.send(own_version).await?;

                let peer_version = framed_stream.try_next().await?;
                assert_matches!(peer_version, Some(Message::Version(..)));
            }
            (Some(HandshakeKind::VersionOnly), ConnectionSide::Responder) => {
                // Receive and send Version.
                let peer_version = framed_stream.try_next().await?;
                let node_addr = match peer_version {
                    Some(Message::Version(version)) => version.addr_from.addr,
                    Some(other) => {
                        let span = self.node().span().clone();
                        error!(
                            parent: span,
                            "received non-version message during handshake: {:?}", other
                        );
                        panic!("Expected Version, got {:?}", other);
                    }
                    None => return Err(io::ErrorKind::InvalidData.into()),
                };

                let own_version = Message::Version(Version::new(node_addr, own_listening_addr));
                framed_stream.send(own_version).await?;
            }
            (None, _) => {}
        }

        Ok(conn)
    }
}
