use core::fmt;
use std::{
    cmp,
    collections::HashMap,
    fs,
    net::SocketAddr,
    time::{Duration, Instant},
};

use super::network::KnownNode;

const LOG_PATH: &str = "crawler-log.txt";

#[allow(dead_code)]
pub struct NetworkSummary {
    num_known_nodes: usize,
    num_good_nodes: usize,
    protocol_versions: HashMap<u32, usize>,
    user_agents: HashMap<String, usize>,
    crawler_runtime: Duration,
}

impl NetworkSummary {
    /// Constructs a new NetworkSummary from given nodes.
    pub fn new(
        nodes: HashMap<SocketAddr, KnownNode>,
        crawler_start_time: Instant,
    ) -> NetworkSummary {
        let num_known_nodes = nodes.len();

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

        NetworkSummary {
            num_known_nodes,
            num_good_nodes,
            protocol_versions,
            user_agents,
            crawler_runtime,
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
