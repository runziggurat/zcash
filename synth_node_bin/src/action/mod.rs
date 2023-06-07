use std::{
    fmt::{self, Display},
    net::{IpAddr, Ipv4Addr, SocketAddr},
    str::FromStr,
};

use anyhow::Result;
use pea2pea::Config as NodeConfig;
use ziggurat_zcash::tools::{message_filter::MessageFilter, synthetic_node::SyntheticNode};

mod advanced_sn_for_s001;
mod quick_connect_and_then_clean_disconnect;
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
#[derive(Clone, Copy)]
pub enum ActionType {
    SendGetAddrAndForeverSleep,
    AdvancedSnForS001,
    QuickConnectAndThenCleanDisconnect,
}

impl Display for ActionType {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(
            f,
            "{}",
            match self {
                Self::SendGetAddrAndForeverSleep => "SendGetAddrAndForeverSleep",
                Self::AdvancedSnForS001 => "AdvancedSnForS001",
                Self::QuickConnectAndThenCleanDisconnect => "QuickConnectAndThenCleanDisconnect",
            }
        )
    }
}

impl FromStr for ActionType {
    type Err = &'static str;

    fn from_str(s: &str) -> Result<Self, Self::Err> {
        match s {
            "SendGetAddrAndForeverSleep" => Ok(Self::SendGetAddrAndForeverSleep),
            "AdvancedSnForS001" => Ok(Self::AdvancedSnForS001),
            "QuickConnectAndThenCleanDisconnect" => Ok(Self::QuickConnectAndThenCleanDisconnect),
            _ => Err("Invalid action type"),
        }
    }
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
            ActionType::QuickConnectAndThenCleanDisconnect => {
                quick_connect_and_then_clean_disconnect::action()
            }
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
