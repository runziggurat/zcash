use std::{fs, net::SocketAddr};

use anyhow::Result;
use rand::{
    distributions::{Distribution, Uniform},
    rngs::StdRng,
    SeedableRng,
};
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

const BLOCKS_FOR_AVG: u128 = 100;

#[async_trait::async_trait]
impl SynthNodeAction for Action {
    fn info(&self) -> &str {
        "constantly ask for random blocks using getdata command"
    }

    fn config(&self) -> ActionCfg {
        ActionCfg::default()
    }

    #[allow(unused_variables)]
    async fn run(&self, synth_node: &mut SyntheticNode, addr: Option<SocketAddr>) -> Result<()> {
        println!("Synthetic node performs an action.");

        let addr = if let Some(addr) = addr {
            addr
        } else {
            anyhow::bail!("address not provided");
        };

        let mut min = u128::MAX;
        let mut max = 0;
        let mut avg = 0;
        let mut count = 0;

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
            let start = std::time::Instant::now();

            loop {
                let (_, msg) = synth_node.try_recv_message().await?;
                tracing::info!("message received: {msg:?}");
                match msg {
                    Message::Block(block) => {
                        let end = std::time::Instant::now();
                        let elapsed = end - start;
                        let elapsed = elapsed.as_millis();
                        if elapsed > max {
                            max = elapsed;
                        }
                        if elapsed < min {
                            min = elapsed;
                        }

                        avg = avg + elapsed;

                        count += 1;

                        if count == BLOCKS_FOR_AVG {
                            println!(
                                "min: {} ms, max: {} ms, avg: {} ms",
                                min,
                                max,
                                avg / BLOCKS_FOR_AVG
                            );
                            min = u128::MAX;
                            max = 0;
                            avg = 0;
                            count = 0;
                        }
                        break;
                    }
                    _ => continue,
                }
            }
        }
    }
}
