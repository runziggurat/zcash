use core::fmt;
use std::{cmp, collections::HashMap, fs, net::SocketAddr, time::Duration};

use md5;
use serde::Serialize;
use spectre::{
    edge::Edge,
    graph::{AGraph, Graph},
};

use crate::{network::LAST_SEEN_CUTOFF, Crawler};

const LOG_PATH: &str = "crawler-log.txt";

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
        NetworkSummary::new(crawler, &mut self.graph)
    }
}

#[allow(dead_code)]
#[derive(Default, Clone, Serialize)]
pub struct NetworkSummary {
    num_known_nodes: usize,
    num_good_nodes: usize,
    num_known_connections: usize,
    num_versions: usize,
    protocol_versions: HashMap<u32, usize>,
    user_agents: HashMap<String, usize>,
    crawler_runtime: Duration,
    node_ids: Vec<String>,
    agraph: AGraph,
}

impl NetworkSummary {
    /// Constructs a new NetworkSummary from given nodes.
    pub fn new(crawler: &Crawler, graph: &mut Graph<SocketAddr>) -> NetworkSummary {
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
        let mut node_ids: Vec<String> = Vec::new();
        for addr in &good_addresses {
            let digest = md5::compute(addr.to_string());
            let hex: String = format!("{:x}", digest);
            // write out 48 bits for id, 12 chars
            // is enough for our purposes, and can be
            // handled as int in JavaScript
            node_ids.push(hex[..12].to_string());
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
            node_ids,
            agraph,
        }
    }

    /// Logs current state of network to file.
    pub fn log_to_file(&self) -> std::io::Result<()> {
        fs::write(LOG_PATH, self.to_string())?;
        Ok(())
    }
}

impl fmt::Display for NetworkSummary {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        fn print_hashmap<T: fmt::Display>(
            f: &mut fmt::Formatter<'_>,
            counts: &HashMap<T, usize>,
        ) -> fmt::Result {
            let mut vec: Vec<(&T, &usize)> = counts.iter().collect();
            vec.sort_by_key(|(_, count)| cmp::Reverse(*count));

            for (item, count) in &vec {
                writeln!(f, "{}: {}", item, count)?;
            }

            Ok(())
        }

        writeln!(f, "Network summary:\n")?;
        writeln!(f, "Found a total of {} node(s)", self.num_known_nodes)?;
        writeln!(f, "Managed to connect to {} node(s)", self.num_good_nodes)?;
        writeln!(
            f,
            "{} identified themselves with a Version",
            self.num_versions
        )?;
        writeln!(
            f,
            "Nodes have {} known connections between them",
            self.num_known_connections
        )?;

        writeln!(f, "\nProtocol versions:")?;
        print_hashmap(f, &self.protocol_versions)?;
        writeln!(f, "\nUser agents:")?;
        print_hashmap(f, &self.user_agents)?;

        writeln!(
            f,
            "\nCrawler ran for a total of {} minutes",
            self.crawler_runtime.as_secs() / 60
        )?;

        Ok(())
    }
}
