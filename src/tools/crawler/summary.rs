use core::fmt;
use std::{
    cmp,
    collections::{HashMap, HashSet},
    fs,
    net::SocketAddr,
    time::{Duration, Instant},
};

use spectre::{edge::Edge, graph::Graph};
use tracing::info;

use crate::network::{KnownConnection, KnownNode};

const LOG_PATH: &str = "crawler-log.txt";

#[allow(dead_code)]
pub struct NetworkSummary {
    num_known_nodes: usize,
    num_good_nodes: usize,
    num_known_connections: usize,
    protocol_versions: HashMap<u32, usize>,
    user_agents: HashMap<String, usize>,
    crawler_runtime: Duration,
    density: f64,
    degree_centrality_delta: f64,
    avg_degree_centrality: u64,
}

impl NetworkSummary {
    /// Constructs a new NetworkSummary from given nodes.
    pub fn new(
        nodes: HashMap<SocketAddr, KnownNode>,
        connections: HashSet<KnownConnection>,
        crawler_start_time: Instant,
    ) -> NetworkSummary {
        let num_known_nodes = nodes.len();
        let num_known_connections = connections.len();

        let good_nodes: HashMap<_, _> = nodes
            .clone()
            .into_iter()
            .filter(|(_, node)| node.last_connected.is_some())
            .collect();

        let num_good_nodes = good_nodes.len();

        let mut protocol_versions = HashMap::with_capacity(num_known_nodes);
        let mut user_agents = HashMap::with_capacity(num_known_nodes);

        for (_, node) in nodes {
            if let Some(_) = node.protocol_version {
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

        let crawler_runtime = crawler_start_time.elapsed();

        // Create a graph to procure its metrics
        let mut graph = Graph::new();

        for conn in connections {
            graph.insert(Edge::new(conn.a, conn.b));
        }

        let density = graph.density();
        let degree_centrality_delta = graph.degree_centrality_delta();
        let degree_centralities = graph.degree_centrality();
        let avg_degree_centrality = degree_centralities.values().map(|v| *v as u64).sum::<u64>() / degree_centralities.len() as u64;

        NetworkSummary {
            num_known_nodes,
            num_good_nodes,
            num_known_connections,
            protocol_versions,
            user_agents,
            crawler_runtime,
            density,
            degree_centrality_delta,
            avg_degree_centrality,
        }
    }

    /// Logs current state of network to file.
    pub fn log_to_file(&self) -> std::io::Result<()> {
        info!("writing to file");
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
        writeln!(f, "Node(s) have {} known connections between them", self.num_known_connections)?;

        writeln!(f, "\nProtocol versions:")?;
        print_hashmap(f, &self.protocol_versions)?;
        writeln!(f, "\nUser agents:")?;
        print_hashmap(f, &self.user_agents)?;

        writeln!(f, "\nNetwork graph metrics:")?;
        writeln!(f, "Density: {:.4}", self.density)?;
        writeln!(f, "Degree centrality delta: {}", self.degree_centrality_delta)?;
        writeln!(f, "Average degree centrality: {:.4}", self.avg_degree_centrality)?;

        writeln!(
            f,
            "\nCrawler ran for a total of {} minutes",
            self.crawler_runtime.as_secs() / 60
        )?;

        Ok(())
    }
}
