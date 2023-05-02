use std::{
    net::SocketAddr,
    sync::Arc,
    thread,
    time::{Duration, Instant},
};

use clap::{ArgGroup, Parser};
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
use ziggurat_zcash::{protocol::message::Message, wait_until};

use crate::{
    metrics::{NetworkMetrics, ZCASH_P2P_DEFAULT_PORT},
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
#[clap(group(
ArgGroup::new("source_addrs")
.required(true)
.multiple(true)
.args(&["seed_addrs", "dns_seed"])
))]
struct Args {
    /// The initial addresses to connect to
    #[clap(short, long, value_parser, num_args(1..))]
    seed_addrs: Option<Vec<SocketAddr>>,

    /// The initial addresses to connect to
    #[clap(short, long, value_parser, num_args(1..))]
    dns_seed: Option<Vec<String>>,

    /// The main crawling loop interval in seconds
    #[clap(short, long, value_parser, default_value_t = MAIN_LOOP_INTERVAL_SECS)]
    crawl_interval: u64,

    /// If present, start an RPC server at the specified address
    #[clap(short, long, value_parser)]
    rpc_addr: Option<SocketAddr>,
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

#[tokio::main]
async fn main() {
    start_logger(LevelFilter::INFO);
    let args = Args::parse();
    let mut seed_addrs: Vec<SocketAddr> = Vec::new();

    // Create the crawler with the given listener address.
    let crawler = Crawler::new().await;

    if let Some(seeds) = args.seed_addrs {
        seed_addrs.extend_from_slice(&seeds);
    }

    if let Some(dns_seed) = args.dns_seed {
        for seed_str in dns_seed {
            let response = lookup_host(&seed_str);

            if let Ok(response) = response {
                for address in response.iter() {
                    seed_addrs.push(SocketAddr::new(*address, ZCASH_P2P_DEFAULT_PORT)); // DNS addrs use this port
                    info!(parent: crawler.node().span(), "DNS seed {} address added: {}", seed_str, address);
                }
            } else {
                error!(parent: crawler.node().span(), "DNS seed {} lookup failed: {}", seed_str, response.err().unwrap());
            }
        }
    }

    if seed_addrs.is_empty() {
        panic!("seed address not found");
    }

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

            if crawler_clone.connect(addr).await.is_ok() {
                sleep(Duration::from_secs(1)).await;
                let _ = crawler_clone.unicast(addr, Message::GetAddr);
            }
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
                        if crawler_clone.connect(addr).await.is_ok() {
                            sleep(Duration::from_secs(1)).await;
                            let _ = crawler_clone.unicast(addr, Message::GetAddr);
                        }
                    });
                }
            }

            crawler.broadcast(Message::GetAddr).unwrap();

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
