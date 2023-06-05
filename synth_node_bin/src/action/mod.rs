use std::net::{IpAddr, Ipv4Addr, SocketAddr};

use anyhow::Result;
use pea2pea::Config as NodeConfig;
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
    AdvancedSnForS001,
}

/// Action configuration options.
pub struct ActionCfg {
    pub msg_filter: MessageFilter,
    pub network_cfg: NodeConfig,
}

impl Default for ActionCfg {
    fn default() -> Self {
        Self {
            msg_filter: MessageFilter::with_all_auto_reply(),
            network_cfg: NodeConfig {
                listener_ip: Some(IpAddr::V4(Ipv4Addr::LOCALHOST)),
                ..Default::default()
            },
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
