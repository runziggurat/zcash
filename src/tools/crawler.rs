use std::{
    cmp::Ordering,
    collections::{HashMap, HashSet},
    hash::{Hash, Hasher},
    io,
    net::{IpAddr, SocketAddr},
    sync::Arc,
    time::{Duration, Instant},
};

use futures_util::SinkExt;
use parking_lot::RwLock;
use pea2pea::{
    protocols::{Handshake, Reading, Writing},
    Config, Connection, Node as Pea2PeaNode, Pea2Pea,
};
use tokio::time::sleep;
use tokio_util::codec::Framed;
use tracing::*;

use crate::{
    protocol::{
        message::Message,
        payload::{block::Headers, Addr, Version},
    },
    tools::synthetic_node::MessageCodec,
};

/// A node encountered in the network or obtained from one of the peers.
#[derive(Debug, Default, Copy, Clone)]
pub struct KnownNode {
    // The address is omitted, as it's a key in the owning HashMap.
    /// The last time the node was successfully connected to.
    pub last_connected: Option<Instant>,
    /// The time it took to complete a connection.
    pub handshake_time: Option<Duration>,
    /// The number of subsequent connection errors.
    connection_failures: u8,
}

/// A connection found in the network.
#[derive(Debug, Eq, Copy, Clone)]
pub struct KnownConnection {
    /// One of the two sides of a connection.
    pub a: SocketAddr,
    /// The other side of a connection.
    pub b: SocketAddr,
    /// The timestamp of the last time the connection was seen.
    pub last_seen: Instant,
}

impl PartialEq for KnownConnection {
    fn eq(&self, other: &Self) -> bool {
        let (a, b) = (self.a, self.b);
        let (c, d) = (other.a, other.b);

        a == d && b == c || a == c && b == d
    }
}

impl Hash for KnownConnection {
    fn hash<H: Hasher>(&self, state: &mut H) {
        let (a, b) = (self.a, self.b);

        // This ensures the hash is the same for (a, b) as it is for (b, a).
        match a.cmp(&b) {
            Ordering::Greater => {
                b.hash(state);
                a.hash(state);
            }
            _ => {
                a.hash(state);
                b.hash(state);
            }
        }
    }
}

impl KnownConnection {
    pub fn new(a: SocketAddr, b: SocketAddr) -> Self {
        Self {
            a,
            b,
            last_seen: Instant::now(),
        }
    }
}

/// The list of nodes and connections the crawler is aware of.
#[derive(Default)]
pub struct KnownNetwork {
    nodes: RwLock<HashMap<SocketAddr, KnownNode>>,
    connections: RwLock<HashSet<KnownConnection>>,
}

impl KnownNetwork {
    /// Extends the list of known nodes.
    pub fn add_addrs(&self, source: SocketAddr, listening_addrs: &[SocketAddr]) {
        let connections = &mut self.connections.write();
        for addr in listening_addrs {
            connections.insert(KnownConnection::new(source, *addr));
        }
    }

    /// Returns a snapshot of the known connections.
    pub fn connections(&self) -> HashSet<KnownConnection> {
        self.connections.read().clone()
    }

    /// Returns a snapshot of the known nodes.
    pub fn nodes(&self) -> HashMap<SocketAddr, KnownNode> {
        self.nodes.read().clone()
    }

    /// Returns the number of known connections.
    pub fn num_connections(&self) -> usize {
        self.connections.read().len()
    }

    /// Returns the number of known nodes.
    pub fn num_nodes(&self) -> usize {
        self.nodes.read().len()
    }

    /// Updates the list of known nodes based on the known connections.
    pub fn update_nodes(&self) {
        let mut prospect_nodes: HashSet<SocketAddr> = HashSet::new();
        for connection in self.connections() {
            prospect_nodes.insert(connection.a);
            prospect_nodes.insert(connection.b);
        }

        let mut nodes = self.nodes.write();
        for addr in prospect_nodes {
            if !nodes.contains_key(&addr) {
                nodes.insert(addr, KnownNode::default());
            }
        }
    }
}

/// Represents the crawler together with network metrics it has collected.
#[derive(Clone)]
pub struct Crawler {
    node: Pea2PeaNode,
    known_network: Arc<KnownNetwork>,
}

impl Pea2Pea for Crawler {
    fn node(&self) -> &Pea2PeaNode {
        &self.node
    }
}

impl Crawler {
    /// Creates a new instance of the `Crawler` without starting it.
    pub async fn new<T: Into<IpAddr>>(ip: T, port: u16) -> Self {
        let config = Config {
            name: Some("crawler".into()),
            listener_ip: Some(ip.into()),
            desired_listening_port: Some(port),
            allow_random_port: false,
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
}

#[async_trait::async_trait]
impl Handshake for Crawler {
    async fn perform_handshake(&self, mut conn: Connection) -> io::Result<Connection> {
        let conn_addr = conn.addr();
        let own_listening_addr = self.node().listening_addr().unwrap();
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

    fn codec(&self, _addr: SocketAddr) -> Self::Codec {
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

                for addr in listening_addrs {
                    if !(self.node().is_connected(addr) || self.node().is_connecting(addr)) {
                        let crawler = self.clone();
                        tokio::spawn(async move {
                            if crawler.connect(addr).await.is_ok() {
                                sleep(Duration::from_secs(1)).await;
                                let _ = crawler.send_direct_message(addr, Message::GetAddr);
                            }
                        });
                    }
                }
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
            Message::Version(_) => {
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

    fn codec(&self, _addr: SocketAddr) -> Self::Codec {
        Default::default()
    }
}

#[cfg(test)]
mod tests {
    use std::time::Duration;

    use tracing_subscriber::filter::{EnvFilter, LevelFilter};

    use super::*;
    use crate::wait_until;

    fn start_logger(default_level: LevelFilter) {
        let filter = match EnvFilter::try_from_default_env() {
            Ok(filter) => filter
                .add_directive("tokio_util=off".parse().unwrap())
                .add_directive("mio=off".parse().unwrap()),
            _ => EnvFilter::default()
                .add_directive(default_level.into())
                .add_directive("tokio_util=off".parse().unwrap())
                .add_directive("mio=off".parse().unwrap()),
        };

        tracing_subscriber::fmt()
            .with_env_filter(filter)
            .with_target(false)
            .init();
    }

    #[tokio::test(flavor = "multi_thread")]
    #[ignore = "Needs to be moved to a binary"]
    async fn crawler_gets_peers() {
        start_logger(LevelFilter::TRACE);

        // Create the crawler with the given listener address.
        let crawler = Crawler::new([127, 0, 0, 1], 0).await;

        crawler.enable_handshake().await;
        crawler.enable_reading().await;
        crawler.enable_writing().await;

        // The initial IPs to connect to.
        let initial_conns = [];

        for addr in initial_conns {
            let crawler_clone = crawler.clone();
            tokio::spawn(async move {
                let addr = addr.parse().unwrap();
                crawler_clone
                    .known_network
                    .nodes
                    .write()
                    .insert(addr, KnownNode::default());

                if crawler_clone.connect(addr).await.is_ok() {
                    sleep(Duration::from_secs(1)).await;
                    let _ = crawler_clone.send_direct_message(addr, Message::GetAddr);
                }
            });
        }

        // Wait for the connection to be complete.
        wait_until!(Duration::from_secs(3), crawler.node().num_connected() >= 1);

        sleep(Duration::from_secs(1)).await;

        tokio::spawn(async move {
            loop {
                crawler.known_network.update_nodes();

                info!(parent: crawler.node().span(), "asking peers for their peers (connected to {})", crawler.node().num_connected());
                info!(parent: crawler.node().span(), "known addrs: {}", crawler.known_network.num_nodes());

                for (addr, _) in crawler.known_network.nodes() {
                    if !(crawler.node().is_connected(addr) || crawler.node().is_connecting(addr)) {
                        let crawler_clone = crawler.clone();
                        tokio::spawn(async move {
                            if crawler_clone.connect(addr).await.is_ok() {
                                sleep(Duration::from_secs(1)).await;
                                let _ = crawler_clone.send_direct_message(addr, Message::GetAddr);
                            }
                        });
                    }
                }

                crawler.send_broadcast(Message::GetAddr).unwrap();

                sleep(Duration::from_secs(60)).await;
            }
        });

        std::future::pending::<()>().await;
    }
}
