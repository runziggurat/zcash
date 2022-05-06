use crate::protocol::{
    message::Message,
    payload::{codec::Codec, Version},
};
use pea2pea::{
    protocols::{Disconnect, Handshake, Reading, Writing},
    Connection, Node as Pea2PeaNode, Pea2Pea,
};
use std::{io, net::SocketAddr};
use tokio_util::codec::Framed;

pub struct SynthNode;

impl Pea2Pea for SynthNode {
    fn node(&self) -> Pea2PeaNode {}
}

#[async_trait::async_trait]
impl Handshake for SynthNode {
    async fn perform_handshake(&self, mut conn: Connection) -> io::Result<Connection> {
        let node_conn_side = !conn.side();
        let conn_addr = conn.addr();
        let own_listening_addr = self.node().listening_addr().unwrap();
        let mut framed_stream = Framed::new(self.borrow_stream(&mut conn), Codec::default());

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

impl Writing for SynthNode {
    type Codec = Codec;
    type Message = Message;

    fn codec(&self, _addr: SocketAddr) -> Self::Codec {
        Default::default()
    }
}

#[async_trait::async_trait]
impl Reading for SynthNode {
    type Codec = Codec;
    type Message = Message;

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
                self.send_direct_message(source, Message(response.into()))?;
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

#[async_trait::async_trait]
impl Disconnect for SynthNode {
    async fn handle_disconnect(&self, disconnecting_addr: SocketAddr) {}
}
