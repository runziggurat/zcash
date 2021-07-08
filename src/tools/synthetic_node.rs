//! A lightweight node implementation to be used as peers in tests.

use crate::{
    protocol::{
        message::{constants::HEADER_LEN, Message, MessageHeader},
        payload::{codec::Codec, Nonce, Version},
    },
    tools::message_filter::{Filter, MessageFilter},
};

use assert_matches::assert_matches;
use pea2pea::{
    connections::ConnectionSide,
    protocols::{Handshaking, Reading, Writing},
    Connection, KnownPeers, Node, NodeConfig, Pea2Pea,
};
use tokio::{
    sync::mpsc::{self, Receiver, Sender},
    time::timeout,
};
use tracing::*;

use std::{
    io::{self, Cursor, Error, ErrorKind},
    net::{IpAddr, Ipv4Addr, SocketAddr},
    time::Duration,
};

/// An [`Error`](std::error::Error) type for [`SyntheticNode::ping_pong_timeout()`]
pub enum PingPongError {
    /// The connection was aborted during the [Ping](Message::Ping)-[Pong](Message::Pong) exchange.
    ConnectionAborted,
    /// An [io::Error] occurred during the [Ping](Message::Ping)-[Pong](Message::Pong) exchange.
    IoErr(io::Error),
    /// Timeout was exceeded before a [Pong](Message::Pong) was received.
    Timeout(Duration),
    /// A message was received which was not [Pong](Message::Pong), or the [Pong's nonce](Nonce) did not match.
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
pub enum Handshake {
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
    handshake: Option<Handshake>,
    message_filter: MessageFilter,
}

impl Default for SyntheticNodeBuilder {
    fn default() -> Self {
        Self {
            network_config: Some(NodeConfig {
                // Set localhost as the default IP.
                listener_ip: IpAddr::V4(Ipv4Addr::LOCALHOST),
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
        let inner_node = InnerNode::new(node, tx, self.message_filter.clone(), self.handshake);

        // Enable the read and write protocols
        inner_node.enable_reading();
        inner_node.enable_writing();

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

    /// Enables handshaking with [`Handshake::Full`].
    pub fn with_full_handshake(mut self) -> Self {
        self.handshake = Some(Handshake::Full);
        self
    }

    /// Enables handshaking with [`Handshake::VersionOnly`].
    pub fn with_version_exchange_handshake(mut self) -> Self {
        self.handshake = Some(Handshake::VersionOnly);
        self
    }

    /// Sets the node's [`MessageFilter`].
    pub fn with_message_filter(mut self, filter: MessageFilter) -> Self {
        self.message_filter = filter;
        self
    }

    /// Sets the node's write buffer size.
    pub fn with_max_write_buffer_size(mut self, size: usize) -> Self {
        let mut config = self.network_config.unwrap_or_default();
        config.conn_write_buffer_size = size;
        self.network_config = Some(config);
        self
    }
}

/// Conventient abstraction over a `pea2pea` node.
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
        self.inner_node.node().listening_addr()
    }

    /// Connects to the target address.
    ///
    /// If the handshake protocol is enabled it will be executed as well.
    pub async fn connect(&self, target: SocketAddr) -> io::Result<()> {
        self.inner_node.node().connect(target).await?;

        Ok(())
    }

    /// Indicates if the `addr` is registerd as a connected peer.
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

    /// Waits until the node has at least one connection, and
    /// returns its SocketAddr
    pub async fn wait_for_connection(&self) -> SocketAddr {
        const SLEEP: Duration = Duration::from_millis(10);
        loop {
            if let Some(addr) = self.known_peers().read().keys().next() {
                return *addr;
            }

            tokio::time::sleep(SLEEP).await;
        }
    }

    /// Sends a direct message to the target address.
    pub async fn send_direct_message(
        &self,
        target: SocketAddr,
        message: Message,
    ) -> io::Result<()> {
        self.inner_node.send_direct_message(target, message).await?;

        Ok(())
    }

    /// Sends bytes directly to the target address.
    pub async fn send_direct_bytes(&self, target: SocketAddr, data: Vec<u8>) -> io::Result<()> {
        self.inner_node.send_direct_bytes(target, data).await?;

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
    /// Uses polling to check that connection is still alive. [Errors](PingPongError) if:
    /// - a non-[`Pong`] message is received
    /// - a [`Pong`] with a non-matching [`Nonce`] is receives
    /// - the timeout expires
    /// - the connection breaks
    /// - an [io::Error] occurs
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
        if let Err(err) = self
            .send_direct_message(target, Message::Ping(ping_nonce))
            .await
        {
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
    pub fn shut_down(&self) {
        self.inner_node.node().shut_down()
    }
}

impl Drop for SyntheticNode {
    fn drop(&mut self) {
        self.shut_down();
    }
}

#[derive(Clone)]
struct InnerNode {
    node: Node,
    handshake: Option<Handshake>,
    inbound_tx: Sender<(SocketAddr, Message)>,
    message_filter: MessageFilter,
}

impl InnerNode {
    fn new(
        node: Node,
        tx: Sender<(SocketAddr, Message)>,
        message_filter: MessageFilter,
        handshake: Option<Handshake>,
    ) -> Self {
        let node = Self {
            node,
            inbound_tx: tx,
            message_filter,
            handshake,
        };

        if handshake.is_some() {
            node.enable_handshaking();
        }

        node
    }

    async fn send_direct_message(&self, target: SocketAddr, message: Message) -> io::Result<()> {
        let mut payload = vec![];
        let header = message.encode(&mut payload)?;

        // Encode the header and append the message to it.
        let mut buffer = Vec::with_capacity(HEADER_LEN + header.body_length as usize);
        header.encode(&mut buffer)?;
        buffer.append(&mut payload);

        self.node()
            .send_direct_message(target, buffer.into())
            .await?;

        Ok(())
    }

    async fn send_direct_bytes(&self, target: SocketAddr, data: Vec<u8>) -> io::Result<()> {
        self.node.send_direct_message(target, data.into()).await
    }
}

impl Pea2Pea for InnerNode {
    fn node(&self) -> &Node {
        &self.node
    }
}

#[async_trait::async_trait]
impl Reading for InnerNode {
    type Message = Message;

    fn read_message(
        &self,
        _source: SocketAddr,
        buffer: &[u8],
    ) -> io::Result<Option<(Self::Message, usize)>> {
        // Check buffer contains a full header.
        if buffer.len() < HEADER_LEN {
            return Ok(None);
        }

        // Decode header.
        let header_bytes = &buffer[..HEADER_LEN];
        let header = MessageHeader::decode(&mut Cursor::new(header_bytes))?;

        // Check buffer contains the announced message length.
        if buffer.len() < HEADER_LEN + header.body_length as usize {
            return Err(ErrorKind::InvalidData.into());
        }

        // Decode message.
        let mut bytes = Cursor::new(&buffer[HEADER_LEN..][..header.body_length as usize]);
        let message = Message::decode(header.command, &mut bytes)?;

        // Read the position from the cursor.
        Ok(Some((message, HEADER_LEN + bytes.position() as usize)))
    }

    async fn process_message(&self, source: SocketAddr, message: Self::Message) -> io::Result<()> {
        let span = self.node().span().clone();

        debug!(parent: span.clone(), "processing {:?}", message);
        match self.message_filter.message_filter_type(&message) {
            Filter::AutoReply => {
                // Autoreply with the appropriate response.
                let response = self.message_filter.reply_message(&message);

                debug!(parent: span, "auto replying with {:?}", response);
                self.send_direct_message(source, response).await?;
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
    fn write_message(
        &self,
        _target: SocketAddr,
        payload: &[u8],
        buffer: &mut [u8],
    ) -> io::Result<usize> {
        buffer[..payload.len()].copy_from_slice(payload);
        Ok(payload.len())
    }
}

#[async_trait::async_trait]
impl Handshaking for InnerNode {
    async fn perform_handshake(&self, mut conn: Connection) -> io::Result<Connection> {
        match (self.handshake, !conn.side) {
            (Some(Handshake::Full), ConnectionSide::Initiator) => {
                // Possible bug: running zebra node results in internal pea2pea panics:
                //  "thread 'tokio-runtime-worker' panicked at 'internal error: entered unreachable code'"
                // which gets "fixed" by reversing the parameters in Version::new -- no current insight into
                // why this is the case. The panic is triggered by the following code in pea2pea:
                // https://docs.rs/pea2pea/0.20.3/src/pea2pea/node.rs.html#201

                // Send and receive Version.
                Message::Version(Version::new(conn.addr, self.node().listening_addr()))
                    .write_to_stream(conn.writer())
                    .await?;

                let version = Message::read_from_stream(conn.reader()).await?;
                assert_matches!(version, Message::Version(..));

                // Send and receive Verack.
                Message::Verack.write_to_stream(conn.writer()).await?;

                let verack = Message::read_from_stream(conn.reader()).await?;
                assert_matches!(verack, Message::Verack);
            }
            (Some(Handshake::Full), ConnectionSide::Responder) => {
                // Receive and send Version.
                let version = Message::read_from_stream(conn.reader()).await?;
                let node_addr = match version {
                    Message::Version(version) => version.addr_from.addr,
                    other => {
                        let span = self.node().span().clone();
                        error!(
                            parent: span,
                            "received non-version message during handshake: {:?}", other
                        );
                        panic!("Expected Version, got {:?}", other);
                    }
                };

                Message::Version(Version::new(node_addr, self.node().listening_addr()))
                    .write_to_stream(conn.writer())
                    .await?;

                // Receive and send Verack.
                let verack = Message::read_from_stream(conn.reader()).await?;
                assert_matches!(verack, Message::Verack);

                Message::Verack.write_to_stream(conn.writer()).await?;
            }
            (Some(Handshake::VersionOnly), ConnectionSide::Initiator) => {
                Message::Version(Version::new(conn.addr, self.node().listening_addr()))
                    .write_to_stream(conn.writer())
                    .await?;

                let version = Message::read_from_stream(conn.reader()).await?;
                assert_matches!(version, Message::Version(..));
            }
            (Some(Handshake::VersionOnly), ConnectionSide::Responder) => {
                // Receive and send Version.
                let version = Message::read_from_stream(conn.reader()).await?;
                let node_addr = match version {
                    Message::Version(version) => version.addr_from.addr,
                    other => {
                        let span = self.node().span().clone();
                        error!(
                            parent: span,
                            "received non-version message during handshake: {:?}", other
                        );
                        panic!("Expected Version, got {:?}", other);
                    }
                };

                Message::Version(Version::new(node_addr, self.node().listening_addr()))
                    .write_to_stream(conn.writer())
                    .await?;
            }
            (None, _) => {}
        }

        Ok(conn)
    }
}
