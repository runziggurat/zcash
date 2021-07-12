//! High level APIs and types for network messages.

pub mod constants;
#[doc(hidden)]
pub mod stream_io;

use crate::protocol::{
    message::constants::*,
    payload::{
        block::{Block, Headers, LocatorHashes},
        codec::Codec,
        read_n_bytes, Addr, FilterAdd, FilterLoad, Inv, Nonce, Reject, Tx, Version,
    },
};

use sha2::{Digest, Sha256};

use std::io::{self, Cursor, Write};

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
    fn encode(&self, buffer: &mut Vec<u8>) -> io::Result<()> {
        buffer.write_all(&self.magic)?;
        buffer.write_all(&self.command)?;
        buffer.write_all(&self.body_length.to_le_bytes())?;
        buffer.write_all(&self.checksum.to_le_bytes())?;

        Ok(())
    }

    fn decode(bytes: &mut Cursor<&[u8]>) -> io::Result<Self> {
        Ok(MessageHeader {
            magic: read_n_bytes(bytes)?,
            command: read_n_bytes(bytes)?,
            body_length: u32::from_le_bytes(read_n_bytes(bytes)?),
            checksum: u32::from_le_bytes(read_n_bytes(bytes)?),
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

impl Message {
    // FIXME: implement Codec?
    /// Encodes a message into the supplied buffer and returns its header.
    pub fn encode(&self, buffer: &mut Vec<u8>) -> io::Result<MessageHeader> {
        let header = match self {
            Self::Version(version) => {
                version.encode(buffer)?;
                MessageHeader::new(VERSION_COMMAND, buffer)
            }
            Self::Verack => MessageHeader::new(VERACK_COMMAND, buffer),
            Self::Ping(nonce) => {
                nonce.encode(buffer)?;
                MessageHeader::new(PING_COMMAND, buffer)
            }
            Self::Pong(nonce) => {
                nonce.encode(buffer)?;
                MessageHeader::new(PONG_COMMAND, buffer)
            }
            Self::GetAddr => MessageHeader::new(GETADDR_COMMAND, buffer),
            Self::Addr(addr) => {
                addr.encode(buffer)?;
                MessageHeader::new(ADDR_COMMAND, buffer)
            }
            Self::GetHeaders(locator_hashes) => {
                locator_hashes.encode(buffer)?;
                MessageHeader::new(GETHEADERS_COMMAND, buffer)
            }
            Self::Headers(headers) => {
                headers.encode(buffer)?;
                MessageHeader::new(HEADERS_COMMAND, buffer)
            }
            Self::GetBlocks(locator_hashes) => {
                locator_hashes.encode(buffer)?;
                MessageHeader::new(GETBLOCKS_COMMAND, buffer)
            }
            Self::Block(block) => {
                block.encode(buffer)?;
                MessageHeader::new(BLOCK_COMMAND, buffer)
            }
            Self::GetData(inv) => {
                inv.encode(buffer)?;
                MessageHeader::new(GETDATA_COMMAND, buffer)
            }
            Self::Inv(inv) => {
                inv.encode(buffer)?;
                MessageHeader::new(INV_COMMAND, buffer)
            }
            Self::NotFound(inv) => {
                inv.encode(buffer)?;
                MessageHeader::new(NOTFOUND_COMMAND, buffer)
            }
            Self::MemPool => MessageHeader::new(MEMPOOL_COMMAND, buffer),
            Self::Tx(tx) => {
                tx.encode(buffer)?;
                MessageHeader::new(TX_COMMAND, buffer)
            }
            Self::Reject(reject) => {
                reject.encode(buffer)?;
                MessageHeader::new(REJECT_COMMAND, buffer)
            }
            Self::FilterLoad(filter_load) => {
                filter_load.encode(buffer)?;
                MessageHeader::new(FILTERLOAD_COMMAND, buffer)
            }
            Self::FilterAdd(filter) => {
                filter.encode(buffer)?;
                MessageHeader::new(FILTERADD_COMMAND, buffer)
            }
            Self::FilterClear => MessageHeader::new(FILTERCLEAR_COMMAND, buffer),
        };

        Ok(header)
    }

    /// Decodes the bytes into a message.
    pub fn decode(command: [u8; 12], bytes: &mut Cursor<&[u8]>) -> io::Result<Self> {
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
