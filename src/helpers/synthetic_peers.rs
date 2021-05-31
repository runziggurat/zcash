use crate::protocol::{
    message::{Message, MessageHeader, MessageWithHeader},
    payload::{codec::Codec, Version},
};

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

pub struct SyntheticNode {
    inner_node: InnerNode,
    inbound_rx: Receiver<MessageWithHeader>,
}

impl SyntheticNode {
    pub fn new(node: Node, enable_handshaking: bool) -> Self {
        // Inbound channel size of 100 messages.
        let (tx, rx) = mpsc::channel(100);
        let inner_node = InnerNode::new(node, tx);

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

    pub async fn connect(&self, target: SocketAddr) -> Result<()> {
        self.inner_node.node().connect(target).await?;

        Ok(())
    }

    pub async fn send_direct_message(&self, target: SocketAddr, message: Message) -> Result<()> {
        let mut payload = vec![];
        let header = message.encode(&mut payload)?;

        // Encode the header and append the message to it.
        let mut buffer = Vec::with_capacity(24 + header.body_length as usize);
        header.encode(&mut buffer)?;
        buffer.append(&mut payload);

        self.inner_node
            .node()
            .send_direct_message(target, buffer.into())
            .await?;

        Ok(())
    }
}

#[derive(Clone)]
struct InnerNode {
    node: Node,
    inbound_tx: Sender<MessageWithHeader>,
}

impl InnerNode {
    fn new(node: Node, tx: Sender<MessageWithHeader>) -> Self {
        Self {
            node,
            inbound_tx: tx,
        }
    }
}

impl Pea2Pea for InnerNode {
    fn node(&self) -> &Node {
        &self.node
    }
}

#[async_trait::async_trait]
impl Reading for InnerNode {
    type Message = MessageWithHeader;

    fn read_message(
        &self,
        _source: SocketAddr,
        buffer: &[u8],
    ) -> Result<Option<(Self::Message, usize)>> {
        let mut bytes = Cursor::new(buffer);
        let header = MessageHeader::decode(&mut bytes)?;
        let message = Message::decode(&mut bytes)?;

        let message_with_header = MessageWithHeader { header, message };

        Ok(Some((message_with_header, bytes.position() as usize)))
    }

    async fn process_message(&self, source: SocketAddr, message: Self::Message) -> Result<()> {
        // FIXME: implement with message filter.

        if let Err(_) = self.inbound_tx.send(message).await {
            panic!("synthetic node receiver dropped");
        };

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
        buffer.copy_from_slice(&payload);
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
                assert!(matches!(version, Message::Version(..)));

                // Send and receive Verack.
                Message::Verack.write_to_stream(conn.writer()).await?;

                let verack = Message::read_from_stream(conn.reader()).await?;
                assert!(matches!(verack, Message::Verack));
            }

            ConnectionSide::Responder => {
                // Receiev and send Version.
                let version = Message::read_from_stream(conn.reader()).await?;
                assert!(matches!(version, Message::Version(..)));

                Message::Version(Version::new(self.node().listening_addr(), conn.addr))
                    .write_to_stream(conn.writer())
                    .await?;

                // Receieve and send Verack.
                let verack = Message::read_from_stream(conn.reader()).await?;
                assert!(matches!(verack, Message::Verack));

                Message::Verack.write_to_stream(conn.writer()).await?;
            }
        }

        Ok(conn)
    }
}
