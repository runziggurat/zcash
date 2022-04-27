//! High level APIs and types for network messages.

pub mod constants;

use std::io;

use bytes::{Buf, BufMut, BytesMut};
use sha2::{Digest, Sha256};

use crate::protocol::{
    message::constants::*,
    payload::{
        block::{Block, Headers, LocatorHashes},
        codec::Codec,
        Addr, FilterAdd, FilterLoad, Inv, Nonce, Reject, Tx, Version,
    },
};

/// The header of a network message.
#[derive(Debug, Default, Clone)]
pub struct MessageHeader {
    /// The network protocol version.
    pub magic: [u8; 4],
    /// The message command, identifies the type of message being sent.
    pub command: [u8; 12],
    /// The length of the message's body.
    pub body_length: u32,
    /// The checksum of the encoded message body.
    pub checksum: u32,
}

impl Codec for MessageHeader {
    fn encode<B: BufMut>(&self, buffer: &mut B) -> io::Result<()> {
        buffer.put_slice(&self.magic);
        buffer.put_slice(&self.command);
        buffer.put_u32_le(self.body_length);
        buffer.put_u32_le(self.checksum);

        Ok(())
    }

    fn decode<B: Buf>(bytes: &mut B) -> io::Result<Self> {
        if bytes.remaining() < HEADER_LEN {
            return Err(io::ErrorKind::InvalidData.into());
        }

        let mut magic = [0u8; 4];
        let mut command = [0u8; 12];

        bytes.copy_to_slice(&mut magic);
        bytes.copy_to_slice(&mut command);

        Ok(MessageHeader {
            magic,
            command,
            body_length: bytes.get_u32_le(),
            checksum: bytes.get_u32_le(),
        })
    }
}

impl MessageHeader {
    /// Returns a `MessageHeader` constructed from the message body.
    pub fn new(command: [u8; 12], body: &[u8]) -> Self {
        MessageHeader {
            magic: MAGIC,
            command,
            body_length: body.len() as u32,
            checksum: checksum(body),
        }
    }
}

/// A network message.
///
/// All the message types and their payloads are documented by the [Bitcoin protocol
/// documentation](https://en.bitcoin.it/wiki/Protocol_documentation#Message_types).
#[derive(Debug, PartialEq, Clone)]
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

macro_rules! encode_with_header_prefix {
    ($command:expr, $buffer:expr) => {{
        let header = MessageHeader::new($command, &[]);
        header.encode($buffer)?;
    }};

    ($command:expr, $buffer:expr, $payload:expr) => {{
        $payload.encode($buffer)?;
        let serialized_payload = $buffer.split_to($buffer.len()).freeze();
        let header = MessageHeader::new($command, &serialized_payload);
        header.encode($buffer)?;
        $buffer.put_slice(&serialized_payload);
    }};
}

impl Message {
    /// Encodes a message into the supplied buffer and returns its header.
    pub fn encode(&self, buffer: &mut BytesMut) -> io::Result<()> {
        match self {
            Self::Version(version) => {
                encode_with_header_prefix!(VERSION_COMMAND, buffer, version);
            }
            Self::Verack => {
                encode_with_header_prefix!(VERACK_COMMAND, buffer);
            }
            Self::Ping(nonce) => {
                encode_with_header_prefix!(PING_COMMAND, buffer, nonce);
            }
            Self::Pong(nonce) => {
                encode_with_header_prefix!(PONG_COMMAND, buffer, nonce);
            }
            Self::GetAddr => {
                encode_with_header_prefix!(GETADDR_COMMAND, buffer);
            }
            Self::Addr(addr) => {
                encode_with_header_prefix!(ADDR_COMMAND, buffer, addr);
            }
            Self::GetHeaders(locator_hashes) => {
                encode_with_header_prefix!(GETHEADERS_COMMAND, buffer, locator_hashes);
            }
            Self::Headers(headers) => {
                encode_with_header_prefix!(HEADERS_COMMAND, buffer, headers);
            }
            Self::GetBlocks(locator_hashes) => {
                encode_with_header_prefix!(GETBLOCKS_COMMAND, buffer, locator_hashes);
            }
            Self::Block(block) => {
                encode_with_header_prefix!(BLOCK_COMMAND, buffer, block);
            }
            Self::GetData(inv) => {
                encode_with_header_prefix!(GETDATA_COMMAND, buffer, inv);
            }
            Self::Inv(inv) => {
                encode_with_header_prefix!(INV_COMMAND, buffer, inv);
            }
            Self::NotFound(inv) => {
                encode_with_header_prefix!(NOTFOUND_COMMAND, buffer, inv);
            }
            Self::MemPool => {
                encode_with_header_prefix!(MEMPOOL_COMMAND, buffer);
            }
            Self::Tx(tx) => {
                encode_with_header_prefix!(TX_COMMAND, buffer, tx);
            }
            Self::Reject(reject) => {
                encode_with_header_prefix!(REJECT_COMMAND, buffer, reject);
            }
            Self::FilterLoad(filter_load) => {
                encode_with_header_prefix!(FILTERLOAD_COMMAND, buffer, filter_load);
            }
            Self::FilterAdd(filter) => {
                encode_with_header_prefix!(FILTERADD_COMMAND, buffer, filter);
            }
            Self::FilterClear => {
                encode_with_header_prefix!(FILTERCLEAR_COMMAND, buffer);
            }
        }

        Ok(())
    }

    /// Decodes the bytes into a message.
    pub fn decode<B: Buf>(command: [u8; 12], bytes: &mut B) -> io::Result<Self> {
        let message = match command {
            VERSION_COMMAND => Self::Version(Version::decode(bytes)?),
            VERACK_COMMAND => Self::Verack,
            PING_COMMAND => Self::Ping(Nonce::decode(bytes)?),
            PONG_COMMAND => Self::Pong(Nonce::decode(bytes)?),
            GETADDR_COMMAND => Self::GetAddr,
            ADDR_COMMAND => Self::Addr(Addr::decode(bytes)?),
            GETHEADERS_COMMAND => Self::GetHeaders(LocatorHashes::decode(bytes)?),
            HEADERS_COMMAND => Self::Headers(Headers::decode(bytes)?),
            GETBLOCKS_COMMAND => Self::GetBlocks(LocatorHashes::decode(bytes)?),
            BLOCK_COMMAND => Self::Block(Box::new(Block::decode(bytes)?)),
            GETDATA_COMMAND => Self::GetData(Inv::decode(bytes)?),
            INV_COMMAND => Self::Inv(Inv::decode(bytes)?),
            NOTFOUND_COMMAND => Self::NotFound(Inv::decode(bytes)?),
            MEMPOOL_COMMAND => Self::MemPool,
            TX_COMMAND => Self::Tx(Tx::decode(bytes)?),
            REJECT_COMMAND => Self::Reject(Reject::decode(bytes)?),
            cmd => {
                return Err(std::io::Error::new(
                    std::io::ErrorKind::InvalidInput,
                    format!("Unknown command string: {:?}", cmd),
                ))
            }
        };

        Ok(message)
    }
}

impl std::fmt::Display for Message {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Message::Version(_) => f.write_str("Version"),
            Message::Verack => f.write_str("Verack"),
            Message::Ping(nonce) => f.write_fmt(format_args!("Ping({:?})", nonce)),
            Message::Pong(nonce) => f.write_fmt(format_args!("Pong({:?})", nonce)),
            Message::GetAddr => f.write_str("GetAddr"),
            Message::Addr(_) => f.write_str("Addr"),
            Message::GetHeaders(_) => f.write_str("GetHeaders"),
            Message::Headers(_) => f.write_str("Headers"),
            Message::GetBlocks(_) => f.write_str("GetBlocks"),
            Message::Block(_) => f.write_str("Block"),
            Message::GetData(_) => f.write_str("GetData"),
            Message::Inv(_) => f.write_str("Inv"),
            Message::NotFound(_) => f.write_str("NotFound"),
            Message::MemPool => f.write_str("MemPool"),
            Message::Tx(_) => f.write_str("Tx"),
            Message::Reject(reject) => f.write_fmt(format_args!("Reject({:?})", reject.ccode)),
            Message::FilterLoad(_) => f.write_str("FilterLoad"),
            Message::FilterAdd(_) => f.write_str("FilterAdd"),
            Message::FilterClear => f.write_str("FilterClear"),
        }
    }
}

fn checksum(bytes: &[u8]) -> u32 {
    let sha2 = Sha256::digest(bytes);
    let sha2d = Sha256::digest(&sha2);

    let mut checksum = [0u8; 4];
    checksum[0..4].copy_from_slice(&sha2d[0..4]);

    u32::from_le_bytes(checksum)
}
