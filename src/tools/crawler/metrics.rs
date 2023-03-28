use std::{collections::HashMap, net::SocketAddr};

use regex::Regex;
use spectre::{edge::Edge, graph::Graph};
use ziggurat_core_crawler::summary::{NetworkSummary, NetworkType};

use crate::{
    network::{KnownNode, LAST_SEEN_CUTOFF},
    Crawler,
};

const MIN_BLOCK_HEIGHT: i32 = 2_000_000;
const ZCASH_P2P_PORT: u16 = 8233;

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

fn recognize_network_types(
    nodes: &HashMap<SocketAddr, KnownNode>,
    good_nodes: &Vec<SocketAddr>,
) -> Vec<NetworkType> {
    let num_good_nodes = good_nodes.len();
    let mut node_network_types = Vec::with_capacity(num_good_nodes);
    for node in good_nodes {
        let mut agent_matches = false;

        let port_matches = node.port() == ZCASH_P2P_PORT;

        let agent = if let Some(agent) = &nodes[node].user_agent {
            agent.0.clone()
        } else {
            "".to_string()
        };
        let zcash_regex = Regex::new(r"^/MagicBean:(\d)\.(\d)\.(\d)/$").unwrap();
        let zebra_regex = Regex::new(r"^/Zebra:(\d)\.(\d)\.(\d)").unwrap();

        // Look for zcash agent like "/MagicBean:5.4.2/"
        let cap_zc = zcash_regex.captures(agent.as_str());
        if let Some(cap) = cap_zc {
            let major = cap.get(1).unwrap().as_str().parse::<u32>().unwrap();
            if major < 6 {
                // Accept all zcash versions < 6 (6 is Flux)
                agent_matches = true;
            } else if major == 6 {
                // Block all zcash versions 6 (Flux) even if they are on the right port
                node_network_types.push(NetworkType::Unknown);
                continue;
            }
        }

        // Look for zebra agent like "/Zebra:1.0.0-rc.4/"
        let cap_ze = zebra_regex.captures(agent.as_str());
        if cap_ze.is_some() {
            // Accept all zebra versions
            agent_matches = true;
        }

        let height = nodes[node].start_height.unwrap_or(0);
        if height < MIN_BLOCK_HEIGHT {
            node_network_types.push(NetworkType::Unknown);
            continue;
        }

        // Height must match and either port or agent must match to recognize node as zcash node and
        // there were no conditions that explicitly blocks matching.
        if port_matches || agent_matches {
            node_network_types.push(NetworkType::Zcash);
        } else {
            node_network_types.push(NetworkType::Unknown);
        }
    }

    node_network_types
}

/// Constructs a new NetworkSummary from given nodes.
pub fn new_network_summary(crawler: &Crawler, graph: &Graph<SocketAddr>) -> NetworkSummary {
    let nodes = crawler.known_network.nodes();
    let connections = crawler.known_network.connections();

    let num_known_nodes = nodes.len();
    let num_known_connections = connections.len();

    let good_nodes = nodes
        .clone()
        .into_iter()
        .filter_map(|(addr, node)| node.last_connected.map(|_| addr))
        .collect::<Vec<_>>();

    let num_good_nodes = good_nodes.len();

    let mut protocol_versions = HashMap::with_capacity(num_known_nodes);
    let mut user_agents = HashMap::with_capacity(num_known_nodes);

    for (_, node) in nodes.iter() {
        if node.protocol_version.is_some() {
            protocol_versions
                .entry(node.protocol_version.unwrap().0)
                .and_modify(|count| *count += 1)
                .or_insert(1);
            user_agents
                .entry(node.user_agent.clone().unwrap().0)
                .and_modify(|count| *count += 1)
                .or_insert(1);
        }
    }

    let node_network_types = recognize_network_types(&nodes, &good_nodes);

    let num_versions = protocol_versions.values().sum();
    let nodes_indices = graph.get_filtered_adjacency_indices(&good_nodes);

    NetworkSummary {
        num_known_nodes,
        num_good_nodes,
        num_known_connections,
        num_versions,
        protocol_versions,
        user_agents,
        crawler_runtime: crawler.start_time.elapsed(),
        node_addrs: good_nodes,
        node_network_types,
        nodes_indices,
    }
}
