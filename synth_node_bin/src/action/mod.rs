use std::{
    fmt::{self, Display},
    net::{IpAddr, Ipv4Addr, SocketAddr},
    str::FromStr,
};

use anyhow::Result;
use pea2pea::Config as NodeConfig;
use ziggurat_zcash::tools::{message_filter::MessageFilter, synthetic_node::SyntheticNode};

mod advanced_sn_for_s001;
mod constantly_ask_for_random_blocks;
mod quick_connect_and_then_clean_disconnect;
mod quick_connect_with_improper_disconnect;
mod rt_s1_collector;
mod rt_s1_tainter;
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
    async fn run(&self, synth_node: &mut SyntheticNode, addr: Option<SocketAddr>) -> Result<()>;
}

/// List of available actions.
#[derive(Clone, Copy)]
pub enum ActionType {
    SendGetAddrAndForeverSleep,
    AdvancedSnForS001,
    QuickConnectAndThenCleanDisconnect,
    QuickConnectWithImproperDisconnect,
    ConstantlyAskForRandomBlocks,
    RtS1Collector,
    RtS1Tainter,
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
                Self::QuickConnectWithImproperDisconnect => "QuickConnectWithImproperDisconnect",
                Self::ConstantlyAskForRandomBlocks => "ConstantlyAskForRandomBlocks",
                Self::RtS1Collector => "RtS1Collector",
                Self::RtS1Tainter => "RtS1Tainter",
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
            "QuickConnectWithImproperDisconnect" => Ok(Self::QuickConnectWithImproperDisconnect),
            "ConstantlyAskForRandomBlocks" => Ok(Self::ConstantlyAskForRandomBlocks),
            "RtS1Collector" => Ok(Self::RtS1Collector),
            "RtS1Tainter" => Ok(Self::RtS1Tainter),
            _ => Err("Invalid action type"),
        }
    }
}

/// Action configuration options.
pub struct ActionCfg {
    /// A message filter for a synthetic node.
    pub msg_filter: MessageFilter,

    /// Network configuration.
    pub network_cfg: NodeConfig,

    /// When enabled, the shutdown API in synthetic node is skipped.
    pub allow_proper_shutdown: bool,
}

impl Default for ActionCfg {
    fn default() -> Self {
        Self {
            msg_filter: MessageFilter::with_all_auto_reply(),
            network_cfg: NodeConfig {
                listener_ip: Some(IpAddr::V4(Ipv4Addr::LOCALHOST)),
                ..Default::default()
            },
            allow_proper_shutdown: true,
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
            ActionType::QuickConnectWithImproperDisconnect => {
                quick_connect_with_improper_disconnect::action()
            }
            ActionType::ConstantlyAskForRandomBlocks => constantly_ask_for_random_blocks::action(),
            ActionType::RtS1Collector => rt_s1_collector::action(),
            ActionType::RtS1Tainter => rt_s1_tainter::action(),
        };
        let cfg = action.config();

        println!(
            "Running a synth node which performs the following:\n\t{}",
            action.info()
        );

        Self { action, cfg }
    }

    /// Runs the underlying action.
    pub async fn execute(
        &self,
        synth_node: &mut SyntheticNode,
        addr: Option<SocketAddr>,
    ) -> Result<()> {
        self.action.run(synth_node, addr).await
    }
}
