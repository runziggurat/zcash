use crate::protocol::{
    message::{constants::HEADER_LEN, filter::MessageFilter, Message},
    payload::{codec::Codec, Version},
};

use assert_matches::assert_matches;
use pea2pea::{
    connections::ConnectionSide,
    protocols::{Handshaking, Reading, Writing},
    Connection, Node, Pea2Pea,
};
use tokio::sync::mpsc::{self, Receiver, Sender};

use std::{
    io::{Cursor, Result},
    net::SocketAddr,
};

/// Conventient abstraction over the `pea2pea`-backed node to be used in tests.
pub struct SyntheticNode {
    inner_node: InnerNode,
    inbound_rx: Receiver<Message>,
}

impl SyntheticNode {
    /// Creates a new synthetic node from a `pea2pea` node.
    ///
    /// The handshake protocol can also optionally enabled and the message filter must be set for
    /// reads.
    pub fn new(node: Node, enable_handshaking: bool, message_filter: MessageFilter) -> Self {
        // Inbound channel size of 100 messages.
        let (tx, rx) = mpsc::channel(100);
        let inner_node = InnerNode::new(node, tx, message_filter);

        // Enable the read and write protocols, handshake is enabled on a per-case basis.
        inner_node.enable_reading();
        inner_node.enable_writing();

        if enable_handshaking {
            inner_node.enable_handshaking();
        }

        Self {
            inner_node,
            inbound_rx: rx,
        }
    }

    /// Connects to the target address.
    ///
    /// If the handshake protocol is enabled it will be executed as well.
    pub async fn connect(&self, target: SocketAddr) -> Result<()> {
        self.inner_node.node().connect(target).await?;

        Ok(())
    }

    /// Reads a message from the inbound (internal) queue of the node.
    ///
    /// Messages are sent to the queue when unfiltered by the message filter.
    pub async fn recv_message(&mut self) -> Message {
        match self.inbound_rx.recv().await {
            Some(message) => message,
            None => panic!("all senders dropped!"),
        }
    }

    /// Sends a direct message to the target address.
    pub async fn send_direct_message(&self, target: SocketAddr, message: Message) -> Result<()> {
        self.inner_node.send_direct_message(target, message).await?;

        Ok(())
    }
}

#[derive(Clone)]
struct InnerNode {
    node: Node,
    inbound_tx: Sender<Message>,
    message_filter: MessageFilter,
}

impl InnerNode {
    fn new(node: Node, tx: Sender<Message>, message_filter: MessageFilter) -> Self {
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
        let mut bytes = Cursor::new(buffer);
        let message = Message::decode(&mut bytes)?;

        Ok(Some((message, bytes.position() as usize)))
    }

    async fn process_message(&self, source: SocketAddr, message: Self::Message) -> Result<()> {
        if let Some(response) = self.message_filter.reply_message(&message) {
            self.send_direct_message(source, response).await?;
        } else if self.inbound_tx.send(message).await.is_err() {
            panic!("receiver dropped!");
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
        buffer[..payload.len()].copy_from_slice(&payload);
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
