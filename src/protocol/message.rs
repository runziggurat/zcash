use crate::protocol::payload::{Addr, Inv, LocatorHashes, Nonce, Version};

use sha2::{Digest, Sha256};
use tokio::{
    io::{self, AsyncReadExt, AsyncWriteExt},
    net::TcpStream,
};

use std::io::Cursor;

const MAGIC: [u8; 4] = [0xfa, 0x1a, 0xf9, 0xbf];

const VERSION_COMMAND: [u8; 12] = *b"version\0\0\0\0\0";
const VERACK_COMMAND: [u8; 12] = *b"verack\0\0\0\0\0\0";
const PING_COMMAND: [u8; 12] = *b"ping\0\0\0\0\0\0\0\0";
const PONG_COMMAND: [u8; 12] = *b"pong\0\0\0\0\0\0\0\0";
const GETADDR_COMMAND: [u8; 12] = *b"getaddr\0\0\0\0\0";
const ADDR_COMMAND: [u8; 12] = *b"addr\0\0\0\0\0\0\0\0";
const GETHEADERS_COMMAND: [u8; 12] = *b"getheaders\0\0";
const GETBLOCKS_COMMAND: [u8; 12] = *b"getblocks\0\0\0";
const GETDATA_COMMAND: [u8; 12] = *b"getdata\0\0\0\0\0";
const INV_COMMAND: [u8; 12] = *b"inv\0\0\0\0\0\0\0\0\0";
const NOTFOUND_COMMAND: [u8; 12] = *b"notfound\0\0\0\0";
const MEMPOOL_COMMAND: [u8; 12] = *b"mempool\0\0\0\0\0";

#[derive(Debug, Default)]
pub struct MessageHeader {
    magic: [u8; 4],
    command: [u8; 12],
    body_length: u32,
    checksum: u32,
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

    pub async fn write_to_stream(&self, stream: &mut TcpStream) -> io::Result<()> {
        stream.write_all(&self.magic).await?;
        stream.write_all(&self.command).await?;
        stream.write_all(&self.body_length.to_le_bytes()).await?;
        stream.write_all(&self.checksum.to_le_bytes()).await?;

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

pub enum Message {
    Version(Version),
    Verack,
    Ping(Nonce),
    Pong(Nonce),
    GetAddr,
    Addr(Addr),
    GetHeaders(LocatorHashes),
    GetBlocks(LocatorHashes),
    GetData(Inv),
    Inv(Inv),
    NotFound(Inv),
    MemPool,
}

impl Message {
    pub async fn write_to_stream(&self, stream: &mut TcpStream) -> io::Result<()> {
        // Buffer for the message payload.
        let mut buffer = vec![];

        let header = match self {
            Self::Version(version) => {
                version.encode(&mut buffer)?;
                MessageHeader::new(VERSION_COMMAND, &buffer)
            }
            Self::Verack => MessageHeader::new(VERACK_COMMAND, &buffer),
            Self::Ping(nonce) => {
                nonce.encode(&mut buffer)?;
                MessageHeader::new(PING_COMMAND, &buffer)
            }
            Self::Pong(nonce) => {
                nonce.encode(&mut buffer)?;
                MessageHeader::new(PONG_COMMAND, &buffer)
            }
            Self::GetAddr => MessageHeader::new(GETADDR_COMMAND, &buffer),
            Self::Addr(addr) => {
                addr.encode(&mut buffer)?;
                MessageHeader::new(ADDR_COMMAND, &buffer)
            }
            Self::GetHeaders(locator_hashes) => {
                locator_hashes.encode(&mut buffer)?;
                MessageHeader::new(GETHEADERS_COMMAND, &buffer)
            }
            Self::GetBlocks(locator_hashes) => {
                locator_hashes.encode(&mut buffer)?;
                MessageHeader::new(GETBLOCKS_COMMAND, &buffer)
            }
            Self::GetData(inv) => {
                inv.encode(&mut buffer)?;
                MessageHeader::new(GETDATA_COMMAND, &buffer)
            }
            Self::Inv(inv) => {
                inv.encode(&mut buffer)?;
                MessageHeader::new(INV_COMMAND, &buffer)
            }
            Self::NotFound(inv) => {
                inv.encode(&mut buffer)?;
                MessageHeader::new(NOTFOUND_COMMAND, &buffer)
            }
            Self::MemPool => MessageHeader::new(MEMPOOL_COMMAND, &buffer),
        };

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
            GETBLOCKS_COMMAND => Self::GetBlocks(LocatorHashes::decode(&mut bytes)?),
            GETDATA_COMMAND => Self::GetData(Inv::decode(&mut bytes)?),
            INV_COMMAND => Self::Inv(Inv::decode(&mut bytes)?),
            NOTFOUND_COMMAND => Self::NotFound(Inv::decode(&mut bytes)?),
            MEMPOOL_COMMAND => Self::MemPool,
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
