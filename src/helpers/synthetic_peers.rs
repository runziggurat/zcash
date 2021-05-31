use crate::protocol::{
    message::{Message, MessageHeader, MessageWithHeader},
    payload::{codec::Codec, Version},
};

use pea2pea::{
    connections::ConnectionSide,
    protocols::{Handshaking, Reading, Writing},
    Connection, Node, Pea2Pea,
};

use std::{
    io::{Cursor, Result},
    net::SocketAddr,
};

#[derive(Clone)]
struct SyntheticNode {
    node: Node,
}

impl Pea2Pea for SyntheticNode {
    fn node(&self) -> &Node {
        &self.node
    }
}

#[async_trait::async_trait]
impl Reading for SyntheticNode {
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
        Ok(())
    }
}

impl Writing for SyntheticNode {
    fn write_message(
        &self,
        target: SocketAddr,
        payload: &[u8],
        buffer: &mut [u8],
    ) -> Result<usize> {
        buffer.copy_from_slice(&payload);
        Ok(payload.len())
    }
}

#[async_trait::async_trait]
impl Handshaking for SyntheticNode {
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
