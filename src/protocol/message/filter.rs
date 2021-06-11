use crate::protocol::{
    message::Message,
    payload::{block::Headers, Addr},
};

use tokio::{io::Result, net::TcpStream};

/// Controls the filter response of [MessageFilter] to messages it reads.
#[derive(Debug, Clone, Copy, PartialEq)]
pub enum Filter {
    /// Do not filter message
    Disabled,
    /// Filter message
    Enabled,
    /// Filter message and reply with a default response
    AutoReply,
}

/// Provides a wrapper around [Message::read_from_stream] which optionally filters
/// certain Message types, and can send default responses if required.
///
/// This can be used to wait for a Message event that you actually care about,
/// while skipping over spurious Message requests e.g. [Message::Ping].
///
/// Currently supports filters for the following [Message] types:
///     - [Message::Ping]
///     - [Message::GetHeaders]
///     - [Message::GetAddr]
///     - [Message::GetData]
///
/// For a list of responses see the documentation on [MessageFilter::read_from_stream].
///
/// Can optionally log filter events to console, logging is disabled by default.
#[derive(Debug, Clone)]
pub struct MessageFilter {
    ping: Filter,
    getheaders: Filter,
    getaddr: Filter,
    getdata: Filter,
    // todo: inv
    // todo: getblocks
    // todo: mempool
    logging: bool,
}

impl MessageFilter {
    /// Constructs a [MessageFilter] which will filter no messages, and with logging disabled.
    pub fn with_all_disabled() -> Self {
        use Filter::Disabled;

        Self {
            ping: Disabled,
            getheaders: Disabled,
            getaddr: Disabled,
            getdata: Disabled,

            logging: false,
        }
    }

    /// Constructs a [MessageFilter] which will filter all supported message types, and with logging disabled.
    pub fn with_all_enabled() -> Self {
        use Filter::Enabled;

        Self {
            ping: Enabled,
            getheaders: Enabled,
            getaddr: Enabled,
            getdata: Enabled,

            logging: false,
        }
    }

    /// Constructs a [MessageFilter] which will filter and reply to all supported message types, and with logging disabled.
    pub fn with_all_auto_reply() -> Self {
        use Filter::AutoReply;

        Self {
            ping: AutoReply,
            getheaders: AutoReply,
            getaddr: AutoReply,
            getdata: AutoReply,

            logging: false,
        }
    }

    /// Enables logging filter events to console
    pub fn enable_logging(mut self) -> Self {
        self.logging = true;
        self
    }

    /// Sets the [Filter] response for [Message::GetHeaders] messages
    pub fn with_getheaders_filter(mut self, filter: Filter) -> Self {
        self.getheaders = filter;
        self
    }

    /// Sets the [Filter] response for [Message::GetAddr] messages
    pub fn with_getaddr_filter(mut self, filter: Filter) -> Self {
        self.getaddr = filter;
        self
    }

    /// Sets the [Filter] response for [Message::GetData] messages
    pub fn with_getdata_filter(mut self, filter: Filter) -> Self {
        self.getdata = filter;
        self
    }

    /// Sets the [Filter] response for [Message::Ping] messages
    pub fn with_ping_filter(mut self, filter: Filter) -> Self {
        self.ping = filter;
        self
    }

    // sends an appropriate reply in response to the received message
    async fn reply(&self, stream: &mut TcpStream, message: Message) -> Result<()> {
        match message {
            Message::Ping(nonce) => Message::Pong(nonce).write_to_stream(stream).await,
            Message::GetAddr => Message::Addr(Addr::empty()).write_to_stream(stream).await,
            Message::GetHeaders(_) => {
                Message::Headers(Headers::empty())
                    .write_to_stream(stream)
                    .await
            }
            Message::GetData(inv) => Message::NotFound(inv).write_to_stream(stream).await,
            _ => unimplemented!(),
        }
    }

    // FIXME: duplication for refactor.
    pub fn reply_message(&self, message: &Message) -> Message {
        match message {
            Message::Ping(nonce) => Message::Pong(*nonce),
            Message::GetAddr => Message::Addr(Addr::empty()),
            Message::GetHeaders(_) => Message::Headers(Headers::empty()),
            Message::GetData(inv) => Message::NotFound(inv.clone()),
            _ => unimplemented!(),
        }
    }

    // returns the Filter of the message type
    pub fn message_filter_type(&self, message: &Message) -> Filter {
        match message {
            Message::Ping(_) => self.ping,
            Message::GetAddr => self.getaddr,
            Message::GetHeaders(_) => self.getheaders,
            Message::GetData(_) => self.getdata,
            _ => Filter::Disabled,
        }
    }

    /// Reads and filters [Messages](Message) from the stream, returning the first unfiltered [Message].
    ///
    /// Repeatedly reads a [Message] from the stream, and processes it according to the [Filter] setting
    /// for that [Message] type:
    /// - [Filter::Enabled] drops the message
    /// - [Filter::AutoReply] sends an appropriate response and drops the message
    /// - [Filter::Disabled] message is returned
    ///
    /// List of responses:
    /// - [Message::Ping(nonce)](Message::Ping)  => [Message::Pong(nonce)](Message::Pong)
    /// - [Message::GetAddr]      => [Message::Addr](Message::Addr)([Addr::empty()])
    /// - [Message::GetHeaders]   => [Message::Headers](Message::Headers)([Headers::empty()])
    /// - [Message::GetData(inv)](Message::GetData) => [Message::NotFound(inv)](Message::NotFound)
    ///
    /// With logging enabled, it will write filter events to console ([Filter::Enabled] and [Filter::AutoReply]).
    pub async fn read_from_stream(&self, stream: &mut TcpStream) -> Result<Message> {
        loop {
            let message = Message::read_from_stream(stream).await?;

            let filter = self.message_filter_type(&message);

            // store message for logging to console (required here because message gets consumed before we log)
            let log_msg = match (self.logging, filter) {
                (true, Filter::Enabled) => Some(format!("Filtered Message::{:?}", message)),
                (true, Filter::AutoReply) => {
                    Some(format!("Filtered and replied to Message::{:?}", message))
                }
                _ => None,
            };

            match filter {
                Filter::Disabled => return Ok(message),
                Filter::AutoReply => self.reply(stream, message).await?,
                Filter::Enabled => {}
            }

            // log filter event to console
            if let Some(log_msg) = log_msg {
                println!("{}", log_msg);
            }
        }
    }
}
