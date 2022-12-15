use core::fmt;
use ordered_map::OrderedMap;
use std::{cmp, collections::{BTreeMap, HashMap}, fs, net::SocketAddr, time::Duration};
use serde::Serialize;
use spectre::{edge::Edge, graph::Graph};

use crate::{network::LAST_SEEN_CUTOFF, Crawler};
use crate::ngraph::NGraph;

const LOG_PATH: &str = "crawler-log.txt";

#[derive(Default)]
pub struct NetworkMetrics {
    graph: Graph<SocketAddr>,
    ngraph: NGraph<SocketAddr>,
}

impl NetworkMetrics {
    /// Updates the network graph with new connections.
    pub fn update_graph(&mut self, crawler: &Crawler) {
        for conn in crawler.known_network.connections() {
            let edge = Edge::new(conn.a, conn.b);
            if conn.last_seen.elapsed().as_secs() > LAST_SEEN_CUTOFF {
                self.graph.remove(&edge);
                self.ngraph.remove(&edge);
            } else {
                self.ngraph.insert(edge.clone());
                self.graph.insert(edge);
            }
        }
    }

    /// Requests a summary of the network metrics.
    pub fn request_summary(&mut self, crawler: &Crawler) -> NetworkSummary {
        NetworkSummary::new(crawler, &mut self.graph, &mut self.ngraph)
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
    density: f64,
    degree_centrality_delta: f64,
    avg_degree_centrality: u64,
    num_edges: usize,
    num_vertices: usize,
    num_kedges: usize,
    num_kvertices: usize,
    degree_centralities: HashMap<SocketAddr, u32>,
    good_addresses: Vec<SocketAddr>,
    good_centralities: HashMap<SocketAddr, u32>,
    sorted_centralities: BTreeMap<SocketAddr, u32>,
    sorted_degrees: Vec<u32>,
}

impl NetworkSummary {
    /// Constructs a new NetworkSummary from given nodes.
    pub fn new(crawler: &Crawler, graph: &mut Graph<SocketAddr>, ngraph: &mut NGraph<SocketAddr>) -> NetworkSummary {
        let nodes = crawler.known_network.nodes();
        let connections = crawler.known_network.connections();

        let num_known_nodes = nodes.len();
        let num_known_connections = connections.len();
        let num_edges = graph.edge_count();
        let num_vertices = graph.vertex_count();
        let num_kedges = ngraph.edge_count();
        let num_kvertices = ngraph.vertex_count();

        let good_nodes: HashMap<_, _> = nodes
            .clone()
            .into_iter()
            .filter(|(_, node)| node.last_connected.is_some())
            .collect();

        let num_good_nodes = good_nodes.len();
        let good_addresses = good_nodes.keys().cloned().collect();

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

        // Procure metrics from the graph.
        let density = graph.density();
        let degree_centrality_delta = graph.degree_centrality_delta();
        let degree_centralities = graph.degree_centrality();
        let avg_degree_centrality = degree_centralities.values().map(|v| *v as u64).sum::<u64>()
            / degree_centralities.len() as u64;

        fn degree_compare (a: &u32) -> u32 {
            *a
        }
        let mut sorted_centralities : BTreeMap<SocketAddr, u32> = BTreeMap::new();
        let mut ordered : OrderedMap<SocketAddr, u32, u32> = OrderedMap::new(degree_compare);
        for (key, value) in &degree_centralities {
            sorted_centralities.insert(*key, *value);
            ordered.insert(*key, *value);
        }
        //sorted_centralities.sort_by(|a, b| b.1.cmp(a.1));
        let mut sorted_degrees : Vec<u32> = Vec::new();
        //for  (key, value) in ordered {
        //    //sorted_degrees.insert(key, v: value);
        //}
        let descending = ordered.descending_values();
        for  key in descending.into_iter() {
           sorted_degrees.push(*key);
        }


        let mut good_centralities: HashMap<SocketAddr, u32> = HashMap::new();
        for  (key, _value) in good_nodes.into_iter() {
            let centrality = degree_centralities.get(&key);
            good_centralities.insert(key, *centrality.unwrap());
        }

        NetworkSummary {
            num_known_nodes,
            num_good_nodes,
            num_known_connections,
            num_versions,
            protocol_versions,
            user_agents,
            crawler_runtime,
            density,
            degree_centrality_delta,
            avg_degree_centrality,
            num_edges,
            num_vertices,
            num_kedges,
            num_kvertices,
            degree_centralities,
            good_addresses,
            good_centralities,
            sorted_centralities,
            sorted_degrees,
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

        writeln!(f, "\nNetwork graph metrics:")?;
        writeln!(f, "Density: {:.4}", self.density)?;
        writeln!(
            f,
            "Degree centrality delta: {}",
            self.degree_centrality_delta
        )?;
        writeln!(
            f,
            "Average degree centrality: {:.4}",
            self.avg_degree_centrality
        )?;

        writeln!(
            f,
            "\nCrawler ran for a total of {} minutes",
            self.crawler_runtime.as_secs() / 60
        )?;

        Ok(())
    }
}
