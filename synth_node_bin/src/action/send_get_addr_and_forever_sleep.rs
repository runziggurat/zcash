use std::net::SocketAddr;

use anyhow::Result;
use tokio::time::{sleep, Duration};
use ziggurat_zcash::{protocol::message::Message, tools::synthetic_node::SyntheticNode};

use super::{ActionCfg, SynthNodeAction};

pub(super) struct Action;

pub(super) fn action() -> Box<dyn SynthNodeAction> {
    Box::new(Action {})
}

#[async_trait::async_trait]
impl SynthNodeAction for Action {
    fn info(&self) -> &str {
        "request an GetAddr and then sleeps forever"
    }

    fn config(&self) -> ActionCfg {
        ActionCfg::default()
    }

    #[allow(unused_variables)]
    async fn run(&self, synth_node: &mut SyntheticNode, addr: SocketAddr) -> Result<()> {
        println!("Synthetic node performs an action.");

        // Custom code goes here, example:
        sleep(Duration::from_millis(5000)).await;

        let msg = Message::GetAddr;
        tracing::info!("unicast {msg:?}\n");
        if synth_node.unicast(addr, msg.clone()).is_err() {
            tracing::warn!("failed to send {msg:?}\n");
            anyhow::bail!("connection closed");
        }

        loop {
            let (_, msg) = synth_node.try_recv_message().await?;
            tracing::info!("message received: {msg:?}");
        }
    }
}
