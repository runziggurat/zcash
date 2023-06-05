use std::{
    net::{IpAddr, Ipv4Addr, SocketAddr},
    str::FromStr,
};

use anyhow::Result;
use pea2pea::Config as NodeConfig;
use tokio::time::{interval, sleep, Duration};
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
        "an advanced SN node created for RT-S001 which can do the following
           - listens for inbound connections and prints the connection status periodically
           - periodically requests and sends Ping/GetAddr/Addr messages to all peers"
    }

    fn config(&self) -> ActionCfg {
        ActionCfg {
            msg_filter: MessageFilter::with_all_auto_reply().with_getaddr_filter(Filter::Disabled),
            network_cfg: NodeConfig {
                listener_ip: Some(IpAddr::V4(Ipv4Addr::UNSPECIFIED)),
                desired_listening_port: Some(8233),
                ..Default::default()
            },
        }
    }

    #[allow(unused_variables)]
    async fn run(&self, synth_node: &mut SyntheticNode, addr: SocketAddr) -> Result<()> {
        // Sleep for three seconds before taking any actions - so the GetHeaders is handled before we
        // send GetAddr to the zcashd node for our only outbound connection.
        sleep(Duration::from_secs(3)).await;

        let msg = Message::GetAddr;
        tracing::info!("unicast {msg:?}\n");
        if synth_node.unicast(addr, msg.clone()).is_err() {
            tracing::warn!("failed to send {msg:?}\n");
            anyhow::bail!("connection closed");
        }

        let mut dbg_info_interval = interval(DBG_INFO_LOG_INTERVAL_SEC);
        let mut broadcast_msgs_interval = interval(BROADCAST_INTERVAL_SEC);

        loop {
            tokio::select! {
                _ = dbg_info_interval.tick() => {
                    trace_debug_info(synth_node).await;
                },
                _ = broadcast_msgs_interval.tick() => {
                    let _ = broadcast_periodic_msgs(synth_node);
                },
                Ok((src, msg)) = synth_node.try_recv_message() => {
                    handle_rx_msg(synth_node, src, msg).await?;
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

        log.push_str(&format!(
            "{addr:>ident$}   ({side:?}) seconds: {time:?}\n",
            addr = addr,
            ident = MAX_IPV6_ADDR_LEN,
            side = info.side(),
            time = stats.created().elapsed()
        ));
    }

    tracing::info!("{log}");
}

fn broadcast_periodic_msgs(synth_node: &mut SyntheticNode) -> Result<()> {
    if let Err(e) = broadcast_ping_msg(synth_node) {
        tracing::warn!("failed to broadcast Ping messages: {e}");
    } else if let Err(e) = broadcast_addr_msg(synth_node) {
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

async fn handle_rx_msg(
    synth_node: &mut SyntheticNode,
    src: SocketAddr,
    msg: Message,
) -> Result<()> {
    tracing::info!("message received from {src}:\n{msg:?}");

    // We are only handling a GetAddr for now.
    if msg == Message::GetAddr {
        unicast_addr_msg(synth_node, src)?;
    }

    Ok(())
}

fn unicast_addr_msg(synth_node: &mut SyntheticNode, dst: SocketAddr) -> Result<()> {
    // These are all our IPs
    let addrs = vec![
        // Our zebrad
        NetworkAddr::new(SocketAddr::from_str("35.210.208.185:8233").unwrap()),
        // Our zcashd
        NetworkAddr::new(SocketAddr::from_str("35.205.233.245:8233").unwrap()),
        // Our synth node
        NetworkAddr::new(SocketAddr::from_str("35.205.233.245:46313").unwrap()),
    ];
    let msg = Message::Addr(Addr::new(addrs));

    tracing::info!("unicast {msg:?}\n");
    if synth_node.unicast(dst, msg.clone()).is_err() {
        tracing::warn!("failed to send {msg:?}\n");
        anyhow::bail!("connection closed");
    }

    Ok(())
}

fn broadcast_addr_msg(synth_node: &mut SyntheticNode) -> Result<()> {
    for addr in synth_node.connected_peers() {
        unicast_addr_msg(synth_node, addr)?;
    }

    Ok(())
}
