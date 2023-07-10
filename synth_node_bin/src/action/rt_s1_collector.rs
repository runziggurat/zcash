use std::net::{IpAddr, Ipv4Addr, SocketAddr};

use anyhow::Result;
use pea2pea::Config as NodeConfig;
use rand::{seq::SliceRandom, thread_rng};
use tokio::time::{interval, Duration};
use ziggurat_zcash::{
    protocol::{
        message::Message,
        payload::{addr::NetworkAddr, Addr},
    },
    tools::{message_filter::MessageFilter, synthetic_node::SyntheticNode},
};

use super::{ActionCfg, SynthNodeAction};

// Configurable status printout interval.
const BROADCAST_INTERVAL_SEC: Duration = Duration::from_secs(60);
const DBG_INFO_LOG_INTERVAL_SEC: Duration = Duration::from_secs(10);

const MAX_PEER_LIST_LEN: usize = 1000;

pub(super) struct Action;

pub(super) fn action() -> Box<dyn SynthNodeAction> {
    Box::new(Action {})
}

#[async_trait::async_trait]
impl SynthNodeAction for Action {
    fn info(&self) -> &str {
        "a node which sends an Addr message every three seconds containing all connected peers"
    }

    fn config(&self) -> ActionCfg {
        ActionCfg {
            msg_filter: MessageFilter::with_all_disabled(),
            network_cfg: NodeConfig {
                listener_ip: Some(IpAddr::V4(Ipv4Addr::UNSPECIFIED)),
                desired_listening_port: Some(18233),
                max_connections: 3000,
                ..Default::default()
            },
            allow_proper_shutdown: true,
        }
    }

    #[allow(unused_variables)]
    async fn run(&self, synth_node: &mut SyntheticNode, addr: Option<SocketAddr>) -> Result<()> {
        let mut broadcast_msgs_interval = interval(BROADCAST_INTERVAL_SEC);
        let mut dbg_info_interval = interval(DBG_INFO_LOG_INTERVAL_SEC);
        let mut num_connected = synth_node.num_connected();

        loop {
            tokio::select! {
                // Print some info about active connections.
                _ = dbg_info_interval.tick() => {
                    trace_debug_info(synth_node).await;
                },
                // Broadcast an Addr message to all peers.
                _ = broadcast_msgs_interval.tick() => {
                    let num_connected_new = synth_node.num_connected();

                    if num_connected == num_connected_new {
                        continue;
                    }

                    num_connected = num_connected_new;
                    let _ = broadcast_addr_msg(synth_node);
                },
                // Clear inbound queue.
                Ok(_) = synth_node.try_recv_message() => (),
            }
        }
    }
}

async fn trace_debug_info(synth_node: &SyntheticNode) {
    let peer_infos = synth_node.connected_peer_infos();
    let peer_cnt = peer_infos.len();

    let mut log = format!("\nNumber of peers: {peer_cnt}\n");

    // Let's sort by the connection's time value.
    let mut peer_infos: Vec<_> = peer_infos.iter().collect();
    peer_infos.sort_by(|a, b| a.1.stats().created().cmp(&b.1.stats().created()));

    for (addr, info) in peer_infos.iter() {
        let stats = info.stats();

        // Align all possible IP addresses (both v4 and v6) vertically
        // Using this value, just like the INET6_ADDRSTRLEN constant in Linux, has 46 bytes
        const MAX_IPV6_ADDR_LEN: usize = 46;

        // Print basic info.
        log.push_str(&format!(
            "{side:?}: {addr:>ident$} - connection established for {time:?}\n\n",
            addr = addr,
            ident = MAX_IPV6_ADDR_LEN,
            side = info.side(),
            time = stats.created().elapsed()
        ));
    }

    tracing::info!("{log}");
}

fn broadcast_addr_msg(synth_node: &mut SyntheticNode) -> Result<()> {
    let mut addrs = synth_node.connected_peers();

    if addrs.len() > MAX_PEER_LIST_LEN {
        addrs.shuffle(&mut thread_rng());
        addrs.truncate(MAX_PEER_LIST_LEN);
    }

    let msg = Message::Addr(Addr::new(
        addrs
            .into_iter()
            .map(|addr| {
                let ip = addr.ip();
                let port = if let Some(hs_info) = synth_node.handshake_info(&addr) {
                    hs_info.addr_from.addr.port()
                } else {
                    // A random choice, this shouldn't ever happen.
                    8233
                };

                NetworkAddr::new(SocketAddr::new(ip, port))
            })
            .collect::<Vec<NetworkAddr>>(),
    ));

    for addr in synth_node.connected_peers() {
        if synth_node.unicast(addr, msg.clone()).is_err() {
            tracing::warn!("failed to send {msg:?}\n");
            anyhow::bail!("connection closed");
        }
    }

    Ok(())
}
