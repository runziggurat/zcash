use std::net::SocketAddr;

use anyhow::Result;
use ziggurat_zcash::tools::synthetic_node::SyntheticNode;

use super::{ActionCfg, SynthNodeAction};

pub(super) struct Action;

pub(super) fn action() -> Box<dyn SynthNodeAction> {
    Box::new(Action {})
}

#[async_trait::async_trait]
impl SynthNodeAction for Action {
    fn info(&self) -> &str {
        "a synth node which only connects and immediately disconnects improperly"
    }

    fn config(&self) -> ActionCfg {
        ActionCfg {
            allow_proper_shutdown: false,
            ..Default::default()
        }
    }

    #[allow(unused_variables)]
    async fn run(&self, synth_node: &mut SyntheticNode, addr: Option<SocketAddr>) -> Result<()> {
        let addr = if let Some(addr) = addr {
            addr
        } else {
            anyhow::bail!("address not provided");
        };

        println!("Synthetic node connected to {addr}!");

        // An optional short sleep.
        //tokio::time::sleep(tokio::time::Duration::from_millis(10)).await;

        println!("Synthetic node disconnecting!");
        Ok(())
    }
}
