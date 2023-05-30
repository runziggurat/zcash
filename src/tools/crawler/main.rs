use std::{
    net::{IpAddr, SocketAddr},
    sync::Arc,
    thread,
    time::{Duration, Instant},
};

use clap::Parser;
use dns_lookup::lookup_host;
use parking_lot::Mutex;
use pea2pea::{
    protocols::{Handshake, Reading, Writing},
    Pea2Pea,
};
use rand::prelude::IteratorRandom;
use tokio::{signal, time::sleep};
use tracing::{debug, error, info, warn};
use tracing_subscriber::filter::{EnvFilter, LevelFilter};
use ziggurat_core_crawler::summary::NetworkSummary;
use ziggurat_zcash::wait_until;

use crate::{
    metrics::{NetworkMetrics, ZCASH_P2P_DEFAULT_MAINNET_PORT},
    network::{ConnectionState, KnownNode},
    protocol::{
        Crawler, MAIN_LOOP_INTERVAL_SECS, MAX_WAIT_FOR_ADDR_SECS, NUM_CONN_ATTEMPTS_PERIODIC,
        RECONNECT_INTERVAL_SECS,
    },
    rpc::{initialize_rpc_server, RpcContext},
};

mod metrics;
mod network;
mod protocol;
mod rpc;

const SEED_WAIT_LOOP_INTERVAL_MS: u64 = 500;
const SEED_RESPONSE_TIMEOUT_MS: u64 = 120_000;
const SUMMARY_LOOP_INTERVAL: u64 = 60;
const LOG_PATH: &str = "crawler-log.txt";

#[derive(Parser)]
#[clap(author, version, about, long_about = None)]
struct Args {
    /// A list of initial standalone IP addresses and/or DNS servers to connect to
    #[clap(short, long, value_parser, num_args(1..), required = true)]
    seed_addrs: Vec<String>,

    /// The main crawling loop interval in seconds
    #[clap(short, long, value_parser, default_value_t = MAIN_LOOP_INTERVAL_SECS)]
    crawl_interval: u64,

    /// If present, start an RPC server at the specified address
    #[clap(short, long, value_parser)]
    rpc_addr: Option<SocketAddr>,

    /// Default port used for connecting to the nodes
    #[clap(short, long, value_parser, default_value_t = ZCASH_P2P_DEFAULT_MAINNET_PORT)]
    node_listening_port: u16,
    // TODO
    // #[clap(short, long, value_parser, default_value = "testnet")]
    // network: String,
}

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

/// Parses and converts `String` values found in a `Vec` to valid `SocketAddr`.
///
/// # Input
///
/// Valid inputs can be in the following forms:
/// - IP + port (both IPv4 and IPv6 are valid)
/// - IP (can be DNS seeder, default_port will be appended)
/// - Hostname + port
/// - Hostname (can be DNS seeder, default_port will be appended)
fn parse_addrs(seed_addrs: Vec<String>, node_listening_port: u16) -> Vec<SocketAddr> {
    let mut parsed_addrs = Vec::with_capacity(seed_addrs.len());

    for seed_addr in seed_addrs {
        // First, try parsing as a `SocketAddr`.
        if let Ok(addr) = seed_addr.parse::<SocketAddr>() {
            parsed_addrs.push(addr);
            continue;
        }
        // User may supply an IP address without a port,
        // append default_port in that case.
        if let Ok(addr) = seed_addr.parse::<IpAddr>() {
            parsed_addrs.push(SocketAddr::new(addr, node_listening_port));
            println!(
                "no port specified for address: {}, using default: {}",
                seed_addr, node_listening_port
            );
            continue;
        }
        // If above failed, try to do a DNS lookup instead.
        //
        // We make sure to remove remove the port, and store it for later use, if it exists.
        // This is safe to do since we catch all IPv6 addresses above.
        let mut clean_addrs = seed_addr.clone();
        let mut addr_split: Vec<_> = seed_addr.split(":").collect();
        let mut port = node_listening_port; // DNS addresses use this port.
        if addr_split.len() > 1 {
            // Port should be the last item, remove it from addrs.
            if let Some(p) = addr_split.pop() {
                port = p.parse().unwrap();
            }
            clean_addrs = addr_split.into_iter().collect();
        }
        // Do the lookup on the clean address.
        let response = lookup_host(&clean_addrs);
        if let Ok(response) = response {
            for address in response.iter() {
                parsed_addrs.push(SocketAddr::new(*address, port));
                println!("DNS seed {} address added: {}", seed_addr, address);
            }
        } else {
            error!("failed to resolve address: {}", seed_addr);
        }
    }

    return parsed_addrs;
}

#[tokio::main]
async fn main() {
    start_logger(LevelFilter::INFO);
    let args = Args::parse();
    let seed_addrs = parse_addrs(args.seed_addrs, args.node_listening_port);

    // Create the crawler with the given listener address.
    let crawler = Crawler::new().await;

    let mut network_metrics = NetworkMetrics::default();
    let summary_snapshot = Arc::new(Mutex::new(NetworkSummary::default()));

    // Initialize the RPC server if address is specified.
    let _rpc_handle = if let Some(addr) = args.rpc_addr {
        let rpc_context = RpcContext::new(Arc::clone(&summary_snapshot));
        let rpc_handle = initialize_rpc_server(addr, rpc_context).await;
        Some(rpc_handle)
    } else {
        None
    };

    crawler.enable_handshake().await;
    crawler.enable_reading().await;
    crawler.enable_writing().await;

    for addr in &seed_addrs {
        let crawler_clone = crawler.clone();
        let addr = *addr;

        tokio::spawn(async move {
            crawler_clone
                .known_network
                .nodes
                .write()
                .insert(addr, KnownNode::default());

            // Once the Version message is received in the process_message function,
            // GetAddr will be requested from the peer
            let _ = crawler_clone.connect(addr).await;
        });
    }

    // Wait for a single successful connection before proceeding.
    wait_until!(Duration::from_secs(3), crawler.node().num_connected() >= 1);

    // Wait for one of the seed nodes to respond with a list of addrs.
    wait_until!(
        Duration::from_millis(SEED_RESPONSE_TIMEOUT_MS),
        crawler.known_network.nodes().len() > seed_addrs.len(),
        Duration::from_millis(SEED_WAIT_LOOP_INTERVAL_MS)
    );

    let crawler_clone = crawler.clone();
    let crawling_loop_task = tokio::spawn(async move {
        let crawler = crawler_clone;
        loop {
            info!(parent: crawler.node().span(), "asking peers for their peers (connected to {})", crawler.node().num_connected());
            info!(parent: crawler.node().span(), "known addrs: {}", crawler.known_network.num_nodes());

            // Filter nodes that stuck in connected state for longer than 3 minutes
            for (addr, _) in crawler
                .known_network
                .nodes()
                .into_iter()
                .filter(|(_, node)| {
                    if node.state == ConnectionState::Connected {
                        if let Some(i) = node.last_connected {
                            i.elapsed().as_secs() >= MAX_WAIT_FOR_ADDR_SECS
                        } else {
                            true
                        }
                    } else {
                        false
                    }
                })
            {
                warn!(parent: crawler.node().span(), "disconnecting from node {} because it didn't send us proper addr message", addr);
                crawler.node().disconnect(addr).await;
                crawler
                    .known_network
                    .set_node_state(addr, ConnectionState::Disconnected);
            }

            for (addr, _) in crawler
                .known_network
                .nodes()
                .into_iter()
                .filter(|(_, node)| {
                    if let Some(i) = node.last_connected {
                        i.elapsed().as_secs() >= RECONNECT_INTERVAL_SECS
                    } else {
                        true
                    }
                })
                .choose_multiple(&mut rand::thread_rng(), NUM_CONN_ATTEMPTS_PERIODIC)
            {
                if crawler.should_connect(addr) {
                    let crawler_clone = crawler.clone();
                    tokio::spawn(async move {
                        // Once the Version message is received in the process_message function,
                        // GetAddr will be requested from the peer
                        let _ = crawler_clone.connect(addr).await;
                    });
                }
            }

            sleep(Duration::from_secs(args.crawl_interval)).await;
        }
    });

    // Clone crawler and summary before we move them into a new thread.
    let crawler_clone = crawler.clone();
    let summary = Arc::clone(&summary_snapshot);

    thread::spawn(move || {
        loop {
            let start_time = Instant::now();

            if crawler.known_network.num_connections() > 0 {
                crawler.known_network.remove_old_connections();

                // Update graph, then create a summary and log it to a file.
                network_metrics.update_graph(&crawler);
                let new_summary = network_metrics.request_summary(&crawler);

                // Aquire lock and replace old summary snapshot with the newly generated one.
                *summary_snapshot.lock() = new_summary;
            }

            let delta_time =
                Duration::from_secs(SUMMARY_LOOP_INTERVAL).saturating_sub(start_time.elapsed());

            if delta_time.is_zero() {
                warn!(parent: crawler.node().span(), "summary calculation took more time than the loop interval");
            }
            info!(parent: crawler.node().span(), "summary calculation took: {:?}", start_time.elapsed());

            thread::sleep(delta_time);
        }
    });

    // Wait for Ctrl-c signal, then abort crawling task.
    let _ = signal::ctrl_c().await;
    debug!(parent: crawler_clone.node().span(), "interrupt received, exiting process");

    crawling_loop_task.abort();
    let _ = crawling_loop_task.await;
    crawler_clone.node().shut_down().await;

    // Print out summary of network metrics.
    let summary = summary.lock();
    info!(parent: crawler_clone.node().span(), "{}", summary);
    if let Err(e) = summary.log_to_file(LOG_PATH) {
        error!(parent: crawler_clone.node().span(), "couldn't write summary to file: {}", e);
    }
}

#[cfg(test)]
mod tests {
    use std::net::{Ipv4Addr, Ipv6Addr};

    use super::*;

    #[test]
    fn parse_addrs_test() {
        let addrs = vec![
            String::from("[::1]:12345"),
            String::from("2001:0db8:85a3:0000:0000:8a2e:0370:7334"),
            String::from("127.0.0.1"),
            String::from("192.0.2.235:54321"),
        ];
        let parsed_addrs = parse_addrs(addrs);

        let correct_addrs = vec![
            SocketAddr::new(IpAddr::V6(Ipv6Addr::new(0, 0, 0, 0, 0, 0, 0, 1)), 12345),
            SocketAddr::new(
                IpAddr::V6(Ipv6Addr::new(
                    0x2001, 0x0db8, 0x85a3, 0x0000, 0x0000, 0x8a2e, 0x0370, 0x7334,
                )),
                ZCASH_P2P_DEFAULT_MAINNET_PORT,
            ),
            SocketAddr::new(
                IpAddr::V4(Ipv4Addr::new(127, 0, 0, 1)),
                ZCASH_P2P_DEFAULT_MAINNET_PORT,
            ),
            SocketAddr::new(IpAddr::V4(Ipv4Addr::new(192, 0, 2, 235)), 54321),
        ];

        assert_eq!(parsed_addrs, correct_addrs)
    }
}
