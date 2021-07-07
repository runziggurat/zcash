use std::io;

use crate::{
    setup::node::{Action, Node},
    tools::synthetic_node::SyntheticNode,
};

mod handshake;
mod invalid_message;
mod messages;
mod unsolicited_response;

/// Creates a connected [Node] and [SyntheticNode] which have completed the full handshake.
///
/// The [SyntheticNode] initiated the connection.
///
/// The [SyntheticNode]'s MessageFilter has been configured for auto replying.
async fn simple_handshaken_node() -> io::Result<(Node, SyntheticNode)> {
    // Spin up a node instance.
    let mut node = Node::new()?;
    node.initial_action(Action::WaitForConnection)
        .start()
        .await?;

    // Create a synthetic node.
    let synthetic_node = SyntheticNode::builder()
        .with_full_handshake()
        .with_all_auto_reply()
        .build()
        .await?;

    // Connect and initiate the handshake.
    synthetic_node.connect(node.addr()).await?;

    Ok((node, synthetic_node))
}
