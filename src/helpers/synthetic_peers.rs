use crate::protocol::{
    message::{
        constants::HEADER_LEN,
        filter::{Filter, MessageFilter},
        Message, MessageHeader,
    },
    payload::{codec::Codec, Version},
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
    io::{Cursor, Error, ErrorKind, Result},
    net::{IpAddr, Ipv4Addr, SocketAddr},
    time::Duration,
};

#[derive(Debug, Clone)]
pub struct SyntheticNodeConfig {
    pub network_config: Option<NodeConfig>,
    pub enable_handshaking: bool,
    pub message_filter: MessageFilter,
}

impl Default for SyntheticNodeConfig {
    fn default() -> Self {
        Self {
            network_config: Some(NodeConfig {
                // Set localhost as the default IP.
                listener_ip: IpAddr::V4(Ipv4Addr::LOCALHOST),
                ..Default::default()
            }),
            enable_handshaking: false,
            message_filter: MessageFilter::with_all_disabled(),
        }
    }
}

/// Conventient abstraction over the `pea2pea`-backed node to be used in tests.
pub struct SyntheticNode {
    inner_node: InnerNode,
    inbound_rx: Receiver<(SocketAddr, Message)>,
}

impl SyntheticNode {
    /// Creates a new synthetic node from a `pea2pea` node.
    ///
    /// The handshake protocol can also optionally enabled and the message filter must be set for
    /// reads.
    pub async fn new(config: SyntheticNodeConfig) -> Result<Self> {
        // Create the pea2pea node from the config.
        let node = Node::new(config.network_config).await?;

        // Inbound channel size of 100 messages.
        let (tx, rx) = mpsc::channel(100);
        let inner_node = InnerNode::new(node, tx, config.message_filter);

        // Enable the read and write protocols, handshake is enabled on a per-case basis.
        inner_node.enable_reading();
        inner_node.enable_writing();

        if config.enable_handshaking {
            inner_node.enable_handshaking();
        }

        Ok(Self {
            inner_node,
            inbound_rx: rx,
        })
    }

    /// Returns the listening address of the node.
    pub fn listening_addr(&self) -> SocketAddr {
        self.inner_node.node().listening_addr()
    }

    /// Connects to the target address.
    ///
    /// If the handshake protocol is enabled it will be executed as well.
    pub async fn connect(&self, target: SocketAddr) -> Result<()> {
        self.inner_node.node().connect(target).await?;

        Ok(())
    }

    pub fn is_connected(&self, addr: SocketAddr) -> bool {
        self.inner_node.node().is_connected(addr)
    }

    pub fn num_connected(&self) -> usize {
        self.inner_node.node().num_connected()
    }

    pub fn known_peers(&self) -> &KnownPeers {
        self.inner_node.node().known_peers()
    }

    /// Sends a direct message to the target address.
    pub async fn send_direct_message(&self, target: SocketAddr, message: Message) -> Result<()> {
        self.inner_node.send_direct_message(target, message).await?;

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
    // FIXME: use timeout duration instead of hardcoding secs
    pub async fn recv_message_timeout(&mut self, secs: u64) -> Result<(SocketAddr, Message)> {
        match timeout(Duration::from_secs(secs), self.recv_message()).await {
            Ok(message) => Ok(message),
            Err(_e) => Err(Error::new(
                ErrorKind::TimedOut,
                format!("could not read message after {}s", secs),
            )),
        }
    }

    /// Sends a ping, expecting a timely pong response.
    ///
    /// Panics if a correct pong isn't sent before the timeout.
    /// FIXME: write  general `expect_message` macro.
    pub async fn assert_ping_pong(&mut self, target: SocketAddr) {
        use crate::protocol::payload::Nonce;

        let ping_nonce = Nonce::default();
        self.send_direct_message(target, Message::Ping(ping_nonce))
            .await
            .unwrap();

        match timeout(Duration::from_secs(2), self.recv_message()).await {
            Ok((_, message)) => {
                // Recieve pong and verify the nonce matches.
                assert_matches!(message, Message::Pong(pong_nonce) if pong_nonce == ping_nonce)
            }
            Err(e) => panic!("no pong response received: {}", e),
        }
    }

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
    inbound_tx: Sender<(SocketAddr, Message)>,
    message_filter: MessageFilter,
}

impl InnerNode {
    fn new(node: Node, tx: Sender<(SocketAddr, Message)>, message_filter: MessageFilter) -> Self {
        Self {
            node,
            inbound_tx: tx,
            message_filter,
        }
    }

    async fn send_direct_message(&self, target: SocketAddr, message: Message) -> Result<()> {
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
    ) -> Result<Option<(Self::Message, usize)>> {
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

    async fn process_message(&self, source: SocketAddr, message: Self::Message) -> Result<()> {
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
    ) -> Result<usize> {
        buffer[..payload.len()].copy_from_slice(payload);
        Ok(payload.len())
    }
}

#[async_trait::async_trait]
impl Handshaking for InnerNode {
    async fn perform_handshake(&self, mut conn: Connection) -> Result<Connection> {
        match !conn.side {
            ConnectionSide::Initiator => {
                // Send and receive Version.
                Message::Version(Version::new(self.node().listening_addr(), conn.addr))
                    .write_to_stream(conn.writer())
                    .await?;

                let version = Message::read_from_stream(conn.reader()).await?;
                assert_matches!(version, Message::Version(..));

                // Send and receive Verack.
                Message::Verack.write_to_stream(conn.writer()).await?;

                let verack = Message::read_from_stream(conn.reader()).await?;
                assert_matches!(verack, Message::Verack);
            }

            ConnectionSide::Responder => {
                // Receive and send Version.
                let version = Message::read_from_stream(conn.reader()).await?;
                assert_matches!(version, Message::Version(..));

                // FIXME: send the node addr with its listener port, not the ephemeral port
                // created for the connection.
                Message::Version(Version::new(self.node().listening_addr(), conn.addr))
                    .write_to_stream(conn.writer())
                    .await?;

                // Receive and send Verack.
                let verack = Message::read_from_stream(conn.reader()).await?;
                assert_matches!(verack, Message::Verack);

                Message::Verack.write_to_stream(conn.writer()).await?;
            }
        }

        Ok(conn)
    }
}
