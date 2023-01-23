use std::{collections::HashMap, net::SocketAddr};

use spectre::{edge::Edge, graph::Graph};
use ziggurat_core_crawler::summary::NetworkSummary;

use crate::{network::LAST_SEEN_CUTOFF, Crawler};

#[derive(Default)]
pub struct NetworkMetrics {
    graph: Graph<SocketAddr>,
}

impl NetworkMetrics {
    /// Updates the network graph with new connections.
    pub fn update_graph(&mut self, crawler: &Crawler) {
        for conn in crawler.known_network.connections() {
            let edge = Edge::new(conn.a, conn.b);
            if conn.last_seen.elapsed().as_secs() > LAST_SEEN_CUTOFF {
                self.graph.remove(&edge);
            } else {
                self.graph.insert(edge);
            }
        }
    }

    /// Requests a summary of the network metrics.
    pub fn request_summary(&mut self, crawler: &Crawler) -> NetworkSummary {
        new_network_summary(crawler, &self.graph)
    }
}

/// Constructs a new NetworkSummary from given nodes.
pub fn new_network_summary(crawler: &Crawler, graph: &Graph<SocketAddr>) -> NetworkSummary {
    let nodes = crawler.known_network.nodes();
    let connections = crawler.known_network.connections();

    let num_known_nodes = nodes.len();
    let num_known_connections = connections.len();

    let good_nodes: HashMap<_, _> = nodes
        .clone()
        .into_iter()
        .filter(|(_, node)| node.last_connected.is_some())
        .collect();

    let num_good_nodes = good_nodes.len();
    let good_addresses: Vec<SocketAddr> = good_nodes.keys().cloned().collect();
    let mut node_ips: Vec<String> = Vec::new();
    for addr in &good_addresses {
        node_ips.push(addr.ip().to_string());
    }

    let mut protocol_versions = HashMap::with_capacity(num_known_nodes);
    let mut user_agents = HashMap::with_capacity(num_known_nodes);

    for (_, node) in nodes {
        if node.protocol_version.is_some() {
            protocol_versions
                .entry(node.protocol_version.unwrap().0)
                .and_modify(|count| *count += 1)
                .or_insert(1);
            user_agents
                .entry(node.user_agent.unwrap().0)
                .and_modify(|count| *count += 1)
                .or_insert(1);
        }
    }

    let num_versions = protocol_versions.values().sum();
    let crawler_runtime = crawler.start_time.elapsed();
    let agraph = graph.create_agraph(&good_addresses);

    NetworkSummary {
        num_known_nodes,
        num_good_nodes,
        num_known_connections,
        num_versions,
        protocol_versions,
        user_agents,
        crawler_runtime,
        node_ips,
        agraph,
    }
}
