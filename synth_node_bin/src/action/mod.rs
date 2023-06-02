use std::net::SocketAddr;

use anyhow::Result;
use ziggurat_zcash::tools::synthetic_node::SyntheticNode;

mod send_get_addr_and_forever_sleep;

/// Defines properties of any action for a synth node binary.
///
/// It simplifies adding new actions and allows to separate different actions with modules.
#[async_trait::async_trait]
trait SynthNodeAction {
    /// Action description.
    ///
    /// It can be displayed during the runtime.
    fn info(&self) -> &str;

    /// Defines the core action functionality.
    ///
    /// All the program logic happens here.
    async fn run(&self, synth_node: &mut SyntheticNode, addr: SocketAddr) -> Result<()>;
}

/// List of available actions.
pub enum ActionType {
    SendGetAddrAndForeverSleep,
}

/// Action handler.
pub struct ActionHandler {
    /// Internal action.
    action: Box<dyn SynthNodeAction>,
}

impl ActionHandler {
    /// Creates a new [`ActionHandler`] for a given [`ActionType`].
    pub fn new(action_type: ActionType) -> Self {
        Self {
            action: Box::new(match action_type {
                ActionType::SendGetAddrAndForeverSleep => {
                    send_get_addr_and_forever_sleep::Action {}
                }
            }),
        }
    }

    /// Runs the underlying action.
    pub async fn execute(&self, synth_node: &mut SyntheticNode, addr: SocketAddr) -> Result<()> {
        println!(
            "Running a synth node which performs the following:\n\t{}",
            self.action.info()
        );

        self.action.run(synth_node, addr).await
    }
}
