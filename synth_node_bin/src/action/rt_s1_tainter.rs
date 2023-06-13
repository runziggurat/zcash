use std::net::{IpAddr, Ipv4Addr, SocketAddr};

use anyhow::Result;
use pea2pea::Config as NodeConfig;
use tokio::time::{interval, Duration};
use ziggurat_zcash::{
    protocol::{
        message::Message,
        payload::{addr::NetworkAddr, Addr, Nonce},
    },
    tools::{
        message_filter::{Filter, MessageFilter},
        synthetic_node::SyntheticNode,
    },
};

use super::{ActionCfg, SynthNodeAction};

// Configurable status printout interval.
const DBG_INFO_LOG_INTERVAL_SEC: Duration = Duration::from_secs(10);
const BROADCAST_INTERVAL_SEC: Duration = Duration::from_secs(120);

pub(super) struct Action;

pub(super) fn action() -> Box<dyn SynthNodeAction> {
    Box::new(Action {})
}

#[async_trait::async_trait]
impl SynthNodeAction for Action {
    fn info(&self) -> &str {
        "a listener node which sends a tainted Addr message to all its peers"
    }

    fn config(&self) -> ActionCfg {
        ActionCfg {
            msg_filter: MessageFilter::with_all_auto_reply().with_getaddr_filter(Filter::Disabled),
            network_cfg: NodeConfig {
                listener_ip: Some(IpAddr::V4(Ipv4Addr::UNSPECIFIED)),
                desired_listening_port: Some(8233),
                ..Default::default()
            },
            allow_proper_shutdown: true,
        }
    }

    #[allow(unused_variables)]
    async fn run(&self, synth_node: &mut SyntheticNode, addr: Option<SocketAddr>) -> Result<()> {
        let addr = if let Some(addr) = addr {
            addr
        } else {
            anyhow::bail!("address not provided");
        };

        let mut dbg_info_interval = interval(DBG_INFO_LOG_INTERVAL_SEC);
        let mut broadcast_msgs_interval = interval(BROADCAST_INTERVAL_SEC);
        let mut tainted_addr_msg = Message::Addr(Addr::new(vec![NetworkAddr::new(
            synth_node.listening_addr(),
        )]));

        loop {
            tokio::select! {
                _ = dbg_info_interval.tick() => {
                    trace_debug_info(synth_node).await;
                },
                _ = broadcast_msgs_interval.tick() => {
                    let _ = broadcast_periodic_msgs(synth_node, &tainted_addr_msg);
                },
                Ok((src, msg)) = synth_node.try_recv_message() => {
                    tracing::info!("message received from {src}:\n{msg:?}");

                    // Store the tainted peer list from our collector.
                    if src == addr && matches!(msg, Message::Addr(_)) {
                        tainted_addr_msg = msg;
                        continue;
                    }

                    // Or otherwise just handle the message.
                    // We are only handling a GetAddr for now.
                    if msg != Message::GetAddr {
                        continue;
                    }

                    if synth_node.unicast(src, tainted_addr_msg.clone()).is_err() {
                        tracing::warn!("failed to send {tainted_addr_msg:?}\n");
                        anyhow::bail!("connection closed");
                    }
                },
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

        // Print some handshake details first - it's easier to see the IP when it it is shown after
        // these details.
        if let Some(hs_info) = synth_node.handshake_info(addr) {
            log.push_str(&format!(
                "{:?} - Services({}) - UserAgent({}) - AddrFrom({}) - Timestamp({}) - StartHeight({})\n",
                hs_info.version, hs_info.services, hs_info.user_agent.0, hs_info.addr_from.addr, hs_info.timestamp, hs_info.start_height
            ));
        }

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

fn broadcast_periodic_msgs(synth_node: &mut SyntheticNode, addr_msg: &Message) -> Result<()> {
    if let Err(e) = broadcast_ping_msg(synth_node) {
        tracing::warn!("failed to broadcast Ping messages: {e}");
    } else if let Err(e) = broadcast_addr_msg(synth_node, addr_msg) {
        tracing::warn!("failed to broadcast Addr messages: {e}");
    } else if let Err(e) = broadcast_get_addr_msg(synth_node) {
        tracing::warn!("failed to broadcast GetAddr messages: {e}");
    }

    Ok(())
}

fn broadcast_ping_msg(synth_node: &mut SyntheticNode) -> Result<()> {
    let msg = Message::Ping(Nonce::default());

    for addr in synth_node.connected_peers() {
        if synth_node.unicast(addr, msg.clone()).is_err() {
            tracing::error!("failed to send {msg:?} to {addr}\n");
            anyhow::bail!("connection closed");
        }
    }

    Ok(())
}

fn broadcast_get_addr_msg(synth_node: &mut SyntheticNode) -> Result<()> {
    let msg = Message::GetAddr;

    for addr in synth_node.connected_peers() {
        if synth_node.unicast(addr, msg.clone()).is_err() {
            tracing::error!("failed to send {msg:?} to {addr}\n");
            anyhow::bail!("connection closed");
        }
    }

    Ok(())
}

fn broadcast_addr_msg(synth_node: &mut SyntheticNode, addr_msg: &Message) -> Result<()> {
    for dst in synth_node.connected_peers() {
        if synth_node.unicast(dst, addr_msg.clone()).is_err() {
            tracing::warn!("failed to send {addr_msg:?}\n");
            anyhow::bail!("connection closed");
        }
    }

    Ok(())
}
