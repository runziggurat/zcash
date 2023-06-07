use std::{fs, net::SocketAddr};

use anyhow::Result;
use rand::{
    distributions::{Distribution, Uniform},
    rngs::StdRng,
    SeedableRng,
};
use tokio::time::{sleep, Duration};
use ziggurat_zcash::{
    protocol::{
        message::Message,
        payload::{inv::InvHash, Hash, Inv},
    },
    tools::synthetic_node::SyntheticNode,
};

use super::{ActionCfg, SynthNodeAction};

pub(super) struct Action;

pub(super) fn action() -> Box<dyn SynthNodeAction> {
    Box::new(Action {})
}

#[async_trait::async_trait]
impl SynthNodeAction for Action {
    fn info(&self) -> &str {
        "constantly ask for random blocks using getdata command"
    }

    fn config(&self) -> ActionCfg {
        ActionCfg::default()
    }

    #[allow(unused_variables)]
    async fn run(&self, synth_node: &mut SyntheticNode, addr: SocketAddr) -> Result<()> {
        println!("Synthetic node performs an action.");

        // Custom code goes here, example:
        let mut rng = StdRng::from_entropy();

        let jstring = fs::read_to_string("hashes.json").expect("could not open hashes file");
        let hashes: Vec<Hash> = serde_json::from_str(&jstring).unwrap();
        let die = Uniform::new(0, hashes.len() - 1);

        loop {
            let msg =
                Message::GetData(Inv::new(vec![InvHash::Block(hashes[die.sample(&mut rng)])]));

            tracing::info!("unicast {msg:?}\n");
            if synth_node.unicast(addr, msg.clone()).is_err() {
                tracing::warn!("failed to send {msg:?}\n");
                anyhow::bail!("connection closed");
            }

            loop {
                let (_, msg) = synth_node.try_recv_message().await?;
                tracing::info!("message received: {msg:?}");
                match msg {
                    Message::Block(block) => break,
                    _ => continue,
                }
            }

            // This sleep is because pushing msgs to the queue at full speed disconnects us from
            // the node.
            sleep(Duration::from_millis(100)).await;
        }
    }
}
