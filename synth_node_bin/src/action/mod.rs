use std::net::SocketAddr;

use anyhow::Result;
use ziggurat_zcash::tools::{message_filter::MessageFilter, synthetic_node::SyntheticNode};

mod advanced_sn_for_s001;
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

    /// Action configuration.
    ///
    /// Allows preconfiguration settings before the action execution starts.
    fn config(&self) -> ActionCfg;

    /// Defines the core action functionality.
    ///
    /// All the program logic happens here.
    async fn run(&self, synth_node: &mut SyntheticNode, addr: SocketAddr) -> Result<()>;
}

/// List of available actions.
// TODO: Make a command argument to choose action type.
#[allow(dead_code)]
pub enum ActionType {
    SendGetAddrAndForeverSleep,
    // TODO(Rqnsom): Add support for choosing listening address in config and apply it in the main.rs here. Details:
    // To use this Action, use:
    //    listener_ip: Some(IpAddr::V4(Ipv4Addr::UNSPECIFIED)),
    //    desired_listening_port: Some(8233),
    // on lines here:
    // https://github.com/runziggurat/zcash/blob/8c0985a87a19d2f3c9cfb10b5d3137e144a27928/src/tools/synthetic_node.rs#L149
    AdvancedSnForS001,
}

/// Action configuration options.
pub struct ActionCfg {
    pub msg_filter: MessageFilter,
}

impl Default for ActionCfg {
    fn default() -> Self {
        Self {
            msg_filter: MessageFilter::with_all_auto_reply(),
        }
    }
}

/// Action handler.
pub struct ActionHandler {
    /// Internal action.
    action: Box<dyn SynthNodeAction>,

    /// Action startup configuration.
    pub cfg: ActionCfg,
}

impl ActionHandler {
    /// Creates a new [`ActionHandler`] for a given [`ActionType`].
    pub fn new(action_type: ActionType) -> Self {
        let action = match action_type {
            ActionType::SendGetAddrAndForeverSleep => send_get_addr_and_forever_sleep::action(),
            ActionType::AdvancedSnForS001 => advanced_sn_for_s001::action(),
        };
        let cfg = action.config();

        println!(
            "Running a synth node which performs the following:\n\t{}",
            action.info()
        );

        Self { action, cfg }
    }

    /// Runs the underlying action.
    pub async fn execute(&self, synth_node: &mut SyntheticNode, addr: SocketAddr) -> Result<()> {
        self.action.run(synth_node, addr).await
    }
}
