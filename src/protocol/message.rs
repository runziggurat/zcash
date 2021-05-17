use crate::protocol::payload::{
    block::{Block, Headers, LocatorHashes},
    codec::Codec,
    Addr, FilterAdd, FilterLoad, Inv, Nonce, Reject, Tx, Version,
};

use sha2::{Digest, Sha256};
use tokio::{
    io::{self, AsyncReadExt},
    net::TcpStream,
};

use std::io::{Cursor, Write};

pub const HEADER_LEN: usize = 24;
pub const MAX_MESSAGE_LEN: usize = 2 * 1024 * 1024;

const MAGIC: [u8; 4] = [0xfa, 0x1a, 0xf9, 0xbf];

pub const VERSION_COMMAND: [u8; 12] = *b"version\0\0\0\0\0";
pub const VERACK_COMMAND: [u8; 12] = *b"verack\0\0\0\0\0\0";
pub const PING_COMMAND: [u8; 12] = *b"ping\0\0\0\0\0\0\0\0";
pub const PONG_COMMAND: [u8; 12] = *b"pong\0\0\0\0\0\0\0\0";
pub const GETADDR_COMMAND: [u8; 12] = *b"getaddr\0\0\0\0\0";
pub const ADDR_COMMAND: [u8; 12] = *b"addr\0\0\0\0\0\0\0\0";
pub const GETHEADERS_COMMAND: [u8; 12] = *b"getheaders\0\0";
pub const HEADERS_COMMAND: [u8; 12] = *b"headers\0\0\0\0\0";
pub const GETBLOCKS_COMMAND: [u8; 12] = *b"getblocks\0\0\0";
pub const BLOCK_COMMAND: [u8; 12] = *b"block\0\0\0\0\0\0\0";
pub const GETDATA_COMMAND: [u8; 12] = *b"getdata\0\0\0\0\0";
pub const INV_COMMAND: [u8; 12] = *b"inv\0\0\0\0\0\0\0\0\0";
pub const NOTFOUND_COMMAND: [u8; 12] = *b"notfound\0\0\0\0";
pub const MEMPOOL_COMMAND: [u8; 12] = *b"mempool\0\0\0\0\0";
pub const TX_COMMAND: [u8; 12] = *b"tx\0\0\0\0\0\0\0\0\0\0";
pub const REJECT_COMMAND: [u8; 12] = *b"reject\0\0\0\0\0\0";
pub const FILTERLOAD_COMMAND: [u8; 12] = *b"filterload\0\0";
pub const FILTERADD_COMMAND: [u8; 12] = *b"filteradd\0\0\0";
pub const FILTERCLEAR_COMMAND: [u8; 12] = *b"filterclear\0";

#[derive(Debug, Default)]
pub struct MessageHeader {
    magic: [u8; 4],
    command: [u8; 12],
    pub body_length: u32,
    pub checksum: u32,
}

impl MessageHeader {
    pub fn new(command: [u8; 12], body: &[u8]) -> Self {
        MessageHeader {
            magic: MAGIC,
            command,
            body_length: body.len() as u32,
            checksum: checksum(body),
        }
    }

    pub fn encode(&self, buffer: &mut Vec<u8>) -> io::Result<()> {
        buffer.write_all(&self.magic)?;
        buffer.write_all(&self.command)?;
        buffer.write_all(&self.body_length.to_le_bytes())?;
        buffer.write_all(&self.checksum.to_le_bytes())?;

        Ok(())
    }

    pub async fn write_to_stream(&self, stream: &mut TcpStream) -> io::Result<()> {
        use tokio::io::AsyncWriteExt;

        let mut buffer = Vec::with_capacity(24);
        self.encode(&mut buffer)?;

        stream.write_all(&buffer).await?;

        Ok(())
    }

    pub async fn read_from_stream(stream: &mut TcpStream) -> io::Result<Self> {
        let mut header: MessageHeader = Default::default();

        stream.read_exact(&mut header.magic).await?;
        stream.read_exact(&mut header.command).await?;
        header.body_length = stream.read_u32_le().await?;
        header.checksum = stream.read_u32_le().await?;

        Ok(header)
    }
}

#[derive(Debug, PartialEq)]
pub enum Message {
    Version(Version),
    Verack,
    Ping(Nonce),
    Pong(Nonce),
    GetAddr,
    Addr(Addr),
    GetHeaders(LocatorHashes),
    Headers(Headers),
    GetBlocks(LocatorHashes),
    Block(Box<Block>),
    GetData(Inv),
    Inv(Inv),
    NotFound(Inv),
    MemPool,
    Tx(Tx),
    Reject(Reject),
    FilterLoad(FilterLoad),
    FilterAdd(FilterAdd),
    FilterClear,
}

impl Message {
    pub fn encode(&self, buffer: &mut Vec<u8>) -> io::Result<MessageHeader> {
        let header = match self {
            Self::Version(version) => {
                version.encode(buffer)?;
                MessageHeader::new(VERSION_COMMAND, &buffer)
            }
            Self::Verack => MessageHeader::new(VERACK_COMMAND, &buffer),
            Self::Ping(nonce) => {
                nonce.encode(buffer)?;
                MessageHeader::new(PING_COMMAND, &buffer)
            }
            Self::Pong(nonce) => {
                nonce.encode(buffer)?;
                MessageHeader::new(PONG_COMMAND, &buffer)
            }
            Self::GetAddr => MessageHeader::new(GETADDR_COMMAND, &buffer),
            Self::Addr(addr) => {
                addr.encode(buffer)?;
                MessageHeader::new(ADDR_COMMAND, &buffer)
            }
            Self::GetHeaders(locator_hashes) => {
                locator_hashes.encode(buffer)?;
                MessageHeader::new(GETHEADERS_COMMAND, &buffer)
            }
            Self::Headers(headers) => {
                headers.encode(buffer)?;
                MessageHeader::new(HEADERS_COMMAND, &buffer)
            }
            Self::GetBlocks(locator_hashes) => {
                locator_hashes.encode(buffer)?;
                MessageHeader::new(GETBLOCKS_COMMAND, &buffer)
            }
            Self::Block(block) => {
                block.encode(buffer)?;
                MessageHeader::new(BLOCK_COMMAND, &buffer)
            }
            Self::GetData(inv) => {
                inv.encode(buffer)?;
                MessageHeader::new(GETDATA_COMMAND, &buffer)
            }
            Self::Inv(inv) => {
                inv.encode(buffer)?;
                MessageHeader::new(INV_COMMAND, &buffer)
            }
            Self::NotFound(inv) => {
                inv.encode(buffer)?;
                MessageHeader::new(NOTFOUND_COMMAND, &buffer)
            }
            Self::MemPool => MessageHeader::new(MEMPOOL_COMMAND, &buffer),
            Self::Tx(tx) => {
                tx.encode(buffer)?;
                MessageHeader::new(TX_COMMAND, &buffer)
            }
            Self::Reject(reject) => {
                reject.encode(buffer)?;
                MessageHeader::new(REJECT_COMMAND, &buffer)
            }
            Self::FilterLoad(filter_load) => {
                filter_load.encode(buffer)?;
                MessageHeader::new(FILTERLOAD_COMMAND, &buffer)
            }
            Self::FilterAdd(filter) => {
                filter.encode(buffer)?;
                MessageHeader::new(FILTERADD_COMMAND, &buffer)
            }
            Self::FilterClear => MessageHeader::new(FILTERCLEAR_COMMAND, &buffer),
        };

        Ok(header)
    }

    pub async fn write_to_stream(&self, stream: &mut TcpStream) -> io::Result<()> {
        use tokio::io::AsyncWriteExt;

        // Buffer for the message payload.
        let mut buffer = vec![];
        let header = self.encode(&mut buffer)?;

        header.write_to_stream(stream).await?;
        stream.write_all(&buffer).await?;

        Ok(())
    }

    pub async fn read_from_stream(stream: &mut TcpStream) -> io::Result<Self> {
        let header = MessageHeader::read_from_stream(stream).await?;

        let mut buffer = vec![0u8; header.body_length as usize];
        stream
            .read_exact(&mut buffer[..header.body_length as usize])
            .await?;

        let mut bytes = Cursor::new(&buffer[..]);

        let message = match header.command {
            VERSION_COMMAND => Self::Version(Version::decode(&mut bytes)?),
            VERACK_COMMAND => Self::Verack,
            PING_COMMAND => Self::Ping(Nonce::decode(&mut bytes)?),
            PONG_COMMAND => Self::Pong(Nonce::decode(&mut bytes)?),
            GETADDR_COMMAND => Self::GetAddr,
            ADDR_COMMAND => Self::Addr(Addr::decode(&mut bytes)?),
            GETHEADERS_COMMAND => Self::GetHeaders(LocatorHashes::decode(&mut bytes)?),
            HEADERS_COMMAND => Self::Headers(Headers::decode(&mut bytes)?),
            GETBLOCKS_COMMAND => Self::GetBlocks(LocatorHashes::decode(&mut bytes)?),
            BLOCK_COMMAND => Self::Block(Box::new(Block::decode(&mut bytes)?)),
            GETDATA_COMMAND => Self::GetData(Inv::decode(&mut bytes)?),
            INV_COMMAND => Self::Inv(Inv::decode(&mut bytes)?),
            NOTFOUND_COMMAND => Self::NotFound(Inv::decode(&mut bytes)?),
            MEMPOOL_COMMAND => Self::MemPool,
            TX_COMMAND => Self::Tx(Tx::decode(&mut bytes)?),
            REJECT_COMMAND => Self::Reject(Reject::decode(&mut bytes)?),
            _ => unimplemented!(),
        };

        Ok(message)
    }
}

fn checksum(bytes: &[u8]) -> u32 {
    let sha2 = Sha256::digest(bytes);
    let sha2d = Sha256::digest(&sha2);

    let mut checksum = [0u8; 4];
    checksum[0..4].copy_from_slice(&sha2d[0..4]);

    u32::from_le_bytes(checksum)
}

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
    async fn reply(&self, stream: &mut TcpStream, message: Message) -> io::Result<()> {
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

    // returns the Filter of the message type
    fn message_filter_type(&self, message: &Message) -> Filter {
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
    pub async fn read_from_stream(&self, stream: &mut TcpStream) -> io::Result<Message> {
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
