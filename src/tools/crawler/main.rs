use std::{
    thread,
    time::{Duration, Instant},
};

use clap::Parser;
use pea2pea::{
    protocols::{Handshake, Reading, Writing},
    Pea2Pea,
};
use rand::prelude::IteratorRandom;
use tokio::time::sleep;
use tracing::{error, info, warn};
use tracing_subscriber::filter::{EnvFilter, LevelFilter};
use ziggurat::{protocol::message::Message, wait_until};

use crate::{
    metrics::NetworkMetrics,
    network::KnownNode,
    protocol::{Crawler, MAIN_LOOP_INTERVAL, NUM_CONN_ATTEMPTS_PERIODIC, RECONNECT_INTERVAL},
};

mod metrics;
mod network;
mod protocol;

const SEED_WAIT_LOOP_INTERVAL_MS: u64 = 500;
const SEED_RESPONSE_TIMEOUT_MS: u64 = 120_000;
const SUMMARY_LOOP_INTERVAL: u64 = 60;

#[derive(Parser)]
#[clap(author, version, about, long_about = None)]
struct Args {
    #[clap(short, long, value_parser, min_values = 1)]
    seed_addrs: Vec<String>,
    #[clap(short, long, value_parser, default_value_t = MAIN_LOOP_INTERVAL)]
    crawl_interval: u64,
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
    start_logger(LevelFilter::TRACE);
    let args = Args::parse();

    // Create the crawler with the given listener address.
    let crawler = Crawler::new().await;

    let mut network_metrics = NetworkMetrics::default();

    crawler.enable_handshake().await;
    crawler.enable_reading().await;
    crawler.enable_writing().await;

    for addr in &args.seed_addrs {
        let crawler_clone = crawler.clone();
        let addr = addr.parse().unwrap();

        tokio::spawn(async move {
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

    // Wait for a single successful connection before proceeding.
    wait_until!(Duration::from_secs(3), crawler.node().num_connected() >= 1);

    // Wait for one of the seed nodes to respond with a list of addrs.
    wait_until!(
        Duration::from_millis(SEED_RESPONSE_TIMEOUT_MS),
        crawler.known_network.nodes().len() > args.seed_addrs.len(),
        Duration::from_millis(SEED_WAIT_LOOP_INTERVAL_MS)
    );

    let crawler_clone = crawler.clone();
    tokio::spawn(async move {
        let crawler = crawler_clone;
        loop {
            info!(parent: crawler.node().span(), "asking peers for their peers (connected to {})", crawler.node().num_connected());
            info!(parent: crawler.node().span(), "known addrs: {}", crawler.known_network.num_nodes());

            for (addr, _) in crawler
                .known_network
                .nodes()
                .into_iter()
                .filter(|(_, node)| {
                    if let Some(i) = node.last_connected {
                        i.elapsed().as_secs() >= RECONNECT_INTERVAL
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
                            let _ = crawler_clone.send_direct_message(addr, Message::GetAddr);
                        }
                    });
                }
            }

            crawler.send_broadcast(Message::GetAddr).unwrap();

            sleep(Duration::from_secs(args.crawl_interval)).await;
        }
    });

    thread::spawn(move || {
        loop {
            let start_time = Instant::now();

            if crawler.known_network.num_connections() > 0 {
                crawler.known_network.remove_old_connections();

                // Update graph, then create a summary and log it to a file.
                network_metrics.update_graph(&crawler);
                let network_summary = network_metrics.request_summary(&crawler);

                info!("{}", network_summary);
                if let Err(e) = network_summary.log_to_file() {
                    error!(parent: crawler.node().span(), "Couldn't write summary to file: {}", e);
                }
            }

            let delta_time =
                Duration::from_secs(SUMMARY_LOOP_INTERVAL).saturating_sub(start_time.elapsed());

            if delta_time.is_zero() {
                warn!(parent: crawler.node().span(), "summary calculation took more time than the loop interval");
            }

            thread::sleep(delta_time);
        }
    });

    std::future::pending::<()>().await;
}
