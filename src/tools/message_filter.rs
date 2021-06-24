//! Message filtering types and utilities.

use crate::protocol::{
    message::Message,
    payload::{block::Headers, Addr},
};

use tokio::{io::Result, net::TcpStream};

/// Controls the filter response of [`MessageFilter`] to messages it receives.
#[derive(Debug, Clone, Copy, PartialEq)]
pub enum Filter {
    /// Do not filter message
    Disabled,
    /// Filter message
    Enabled,
    /// Filter message and reply with a default response
    AutoReply,
}

/// A message filter that can map requests to default responses.
///
/// This can be used to wait for a message event that you actually care about,
/// while skipping over spurious requests e.g. [`Ping`].
///
/// Currently supports filters for the following message types:
/// - [`Ping`]
/// - [`GetHeaders`]
/// - [`GetAddr`]
/// - [`GetData`]
///
/// [`Ping`]: Message::Ping
/// [`GetHeaders`]: Message::GetHeaders
/// [`GetAddr`]: Message::GetAddr
/// [`GetData`]: Message::GetData
#[derive(Debug, Clone)]
pub struct MessageFilter {
    ping: Filter,
    getheaders: Filter,
    getaddr: Filter,
    getdata: Filter,
    // todo: inv
    // todo: getblocks
    // todo: mempool
}

impl MessageFilter {
    /// Constructs a `MessageFilter` which will filter no messages.
    pub fn with_all_disabled() -> Self {
        use Filter::Disabled;

        Self {
            ping: Disabled,
            getheaders: Disabled,
            getaddr: Disabled,
            getdata: Disabled,
        }
    }

    /// Constructs a `MessageFilter` which will filter all supported message types.
    pub fn with_all_enabled() -> Self {
        use Filter::Enabled;

        Self {
            ping: Enabled,
            getheaders: Enabled,
            getaddr: Enabled,
            getdata: Enabled,
        }
    }

    /// Constructs a `MessageFilter` which will filter and reply to all supported message types.
    pub fn with_all_auto_reply() -> Self {
        use Filter::AutoReply;

        Self {
            ping: AutoReply,
            getheaders: AutoReply,
            getaddr: AutoReply,
            getdata: AutoReply,
        }
    }

    /// Sets the [`Filter`] response for [`GetHeaders`] messages.
    ///
    /// [`GetHeaders`]: Message::GetHeaders
    pub fn with_getheaders_filter(mut self, filter: Filter) -> Self {
        self.getheaders = filter;
        self
    }

    /// Sets the [`Filter`] response for [`GetAddr`] messages.
    ///
    /// [`GetAddr`]: Message::GetAddr
    pub fn with_getaddr_filter(mut self, filter: Filter) -> Self {
        self.getaddr = filter;
        self
    }

    /// Sets the [`Filter`] response for [`GetData`] messages.
    ///
    /// [`GetData`]: Message::GetData
    pub fn with_getdata_filter(mut self, filter: Filter) -> Self {
        self.getdata = filter;
        self
    }

    /// Sets the [`Filter`] response for [`Ping`] messages.
    ///
    /// [`Ping`]: Message::Ping
    pub fn with_ping_filter(mut self, filter: Filter) -> Self {
        self.ping = filter;
        self
    }

    /// Returns the set [`Filter`] for the message type.
    pub fn message_filter_type(&self, message: &Message) -> Filter {
        match message {
            Message::Ping(_) => self.ping,
            Message::GetAddr => self.getaddr,
            Message::GetHeaders(_) => self.getheaders,
            Message::GetData(_) => self.getdata,
            _ => Filter::Disabled,
        }
    }

    /// Returns the appropriate reply for the message.
    pub fn reply_message(&self, message: &Message) -> Message {
        match message {
            Message::Ping(nonce) => Message::Pong(*nonce),
            Message::GetAddr => Message::Addr(Addr::empty()),
            Message::GetHeaders(_) => Message::Headers(Headers::empty()),
            Message::GetData(inv) => Message::NotFound(inv.clone()),
            _ => unimplemented!(),
        }
    }
}
