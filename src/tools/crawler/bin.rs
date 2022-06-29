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

use crate::{
    protocol::{
        message::Message,
        payload::{block::Headers, Addr, Version},
    },
    tools::synthetic_node::MessageCodec,
};

use super::network::KnownNetwork;

const NUM_CONN_ATTEMPTS_ON_PEERLIST: usize = 100;
#[allow(dead_code)]
const NUM_CONN_ATTEMPTS_PERIODIC: usize = 100;
#[allow(dead_code)]
const MAIN_LOOP_INTERVAL: u64 = 60;

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
    pub async fn new() -> Self {
        let config = Config {
            name: Some("crawler".into()),
            listener_ip: None,
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

                for addr in listening_addrs.into_iter().take(NUM_CONN_ATTEMPTS_ON_PEERLIST) {
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

#[cfg(test)]
mod tests {
    use std::time::Duration;

    use rand::prelude::IteratorRandom;
    use tracing_subscriber::filter::{EnvFilter, LevelFilter};

    use super::*;
    use crate::{
        wait_until,
        tools::crawler::{summary::NetworkSummary, network::KnownNode}
    };

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
        let crawler = Crawler::new().await;

        crawler.enable_handshake().await;
        crawler.enable_reading().await;
        crawler.enable_writing().await;

        // The initial IPs to connect to.
        let initial_conns: [&str; 0] = [];

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

        // Capture the start time of the crawler.
        let crawler_start_time = Instant::now();

        tokio::spawn(async move {
            loop {
                crawler.known_network.update_nodes();

                info!(parent: crawler.node().span(), "asking peers for their peers (connected to {})", crawler.node().num_connected());
                info!(parent: crawler.node().span(), "known addrs: {}", crawler.known_network.num_nodes());

                for (addr, _) in crawler.known_network.nodes().into_iter().choose_multiple(&mut rand::thread_rng(), NUM_CONN_ATTEMPTS_PERIODIC) {
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

                // Create summary and log to file.
                let network_summary = NetworkSummary::new(crawler.known_network.nodes(), crawler_start_time);
                network_summary.log_to_file().unwrap();
                info!("{}", network_summary);

                sleep(Duration::from_secs(MAIN_LOOP_INTERVAL)).await;
            }
        });

        std::future::pending::<()>().await;
    }
}
