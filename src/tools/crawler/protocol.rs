use std::{
    io,
    net::SocketAddr,
    sync::Arc,
    time::{Duration, Instant},
};

use futures_util::SinkExt;
use pea2pea::{
    protocols::{Handshake, Reading, Writing},
    Config, Connection, ConnectionSide, Node as Pea2PeaNode, Pea2Pea,
};
use tokio::time::sleep;
use tokio_util::codec::Framed;
use tracing::*;
use ziggurat::{
    protocol::{
        message::Message,
        payload::{block::Headers, Addr, Version},
    },
    tools::synthetic_node::MessageCodec,
};

use super::network::KnownNetwork;

pub const NUM_CONN_ATTEMPTS_ON_PEERLIST: usize = 100;
pub const NUM_CONN_ATTEMPTS_PERIODIC: usize = 100;
pub const MAIN_LOOP_INTERVAL: u64 = 60;
pub const RECONNECT_INTERVAL: u64 = 5 * 60;

/// Represents the crawler together with network metrics it has collected.
#[derive(Clone)]
pub struct Crawler {
    node: Pea2PeaNode,
    pub known_network: Arc<KnownNetwork>,
}

impl Pea2Pea for Crawler {
    fn node(&self) -> &Pea2PeaNode {
        &self.node
    }
}

impl Crawler {
    /// Creates a new instance of the `Crawler` without starting it.
    pub async fn new() -> Self {
        let config = Config {
            name: Some("crawler".into()),
            listener_ip: None,
            max_connections: 1000,
            ..Default::default()
        };

        Self {
            node: Pea2PeaNode::new(Some(config)).await.unwrap(),
            known_network: Default::default(),
        }
    }

    /// Attempts to connect the crawler to the given address.
    pub async fn connect(&self, addr: SocketAddr) -> io::Result<()> {
        trace!(parent: self.node().span(), "attempting to connect to {}", addr);

        let timestamp = Instant::now();

        let result = self.node.connect(addr).await;

        if let Some(ref mut known_node) = self.known_network.nodes.write().get_mut(&addr) {
            match result {
                Ok(_) => {
                    known_node.connection_failures = 0;
                    known_node.last_connected = Some(timestamp);
                    known_node.handshake_time = Some(timestamp.elapsed());
                }
                Err(_) => {
                    trace!(parent: self.node().span(), "failed to connect to {}", addr);
                    known_node.connection_failures += 1;
                }
            }
        }

        result
    }

    /// Checks to see if crawler should connect to the given address.
    pub fn should_connect(&self, addr: SocketAddr) -> bool {
        if let Some(node) = self.known_network.nodes().get(&addr) {
            // Ensure that there are no active connections with the given addr.
            if self.node().is_connected(addr) || self.node().is_connecting(addr) {
                return false;
            }

            // Ensure that the RECONNECT_INTERVAL is obeyed.
            if let Some(i) = node.last_connected {
                if i.elapsed() < Duration::from_secs(RECONNECT_INTERVAL) {
                    return false;
                }
            }
            
            true
        } else {
            panic!("Logic bug! The crawler should only attempt to connect to known addresses.");
        }
    }
}

#[async_trait::async_trait]
impl Handshake for Crawler {
    async fn perform_handshake(&self, mut conn: Connection) -> io::Result<Connection> {
        let conn_addr = conn.addr();
        let own_listening_addr: SocketAddr = ([127, 0, 0, 1], 0).into();
        let mut framed_stream = Framed::new(self.borrow_stream(&mut conn), MessageCodec::default());

        let own_version = Message::Version(Version::new(conn_addr, own_listening_addr));
        framed_stream.send(own_version).await?;

        Ok(conn)
    }
}

#[async_trait::async_trait]
impl Reading for Crawler {
    type Message = Message;
    type Codec = MessageCodec;

    fn codec(&self, _addr: SocketAddr, _side: ConnectionSide) -> Self::Codec {
        Default::default()
    }

    async fn process_message(&self, source: SocketAddr, message: Self::Message) -> io::Result<()> {
        match message {
            Message::Addr(addr) => {
                info!(parent: self.node().span(), "got {} address(es) from {}", addr.addrs.len(), source);

                let mut listening_addrs = Vec::with_capacity(addr.addrs.len());
                for addr in addr.addrs {
                    listening_addrs.push(addr.addr);
                }
                self.known_network.add_addrs(source, &listening_addrs);

                for addr in listening_addrs
                    .into_iter()
                    .take(NUM_CONN_ATTEMPTS_ON_PEERLIST)
                {
                    if self.should_connect(addr) {
                        let crawler = self.clone();
                        tokio::spawn(async move {
                            if crawler.connect(addr).await.is_ok() {
                                sleep(Duration::from_secs(1)).await;
                                let _ = crawler.send_direct_message(addr, Message::GetAddr);
                            }
                        });
                    }
                }
                self.node().disconnect(source).await;
            }
            Message::Ping(nonce) => {
                let _ = self
                    .send_direct_message(source, Message::Pong(nonce))
                    .unwrap()
                    .await;
            }
            Message::GetAddr => {
                let _ = self
                    .send_direct_message(source, Message::Addr(Addr::empty()))
                    .unwrap()
                    .await;
            }
            Message::GetHeaders(_) => {
                let _ = self
                    .send_direct_message(source, Message::Headers(Headers::empty()))
                    .unwrap()
                    .await;
            }
            Message::GetData(inv) => {
                let _ = self
                    .send_direct_message(source, Message::NotFound(inv.clone()))
                    .unwrap()
                    .await;
            }
            Message::Version(ver) => {
                // Update source node with information from version.
                if let Some(known_node) = self.known_network.nodes.write().get_mut(&source) {
                    known_node.protocol_version = Some(ver.version);
                    known_node.user_agent = Some(ver.user_agent);
                    known_node.services = Some(ver.services);
                }

                let _ = self
                    .send_direct_message(source, Message::Verack)
                    .unwrap()
                    .await;
            }
            _ => {}
        }

        Ok(())
    }
}

impl Writing for Crawler {
    type Message = Message;
    type Codec = MessageCodec;

    fn codec(&self, _addr: SocketAddr, _side: ConnectionSide) -> Self::Codec {
        Default::default()
    }
}
