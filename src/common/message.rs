// use byteorder::{BigEndian, ByteOrder, LittleEndian, WriteBytesExt};
// use bytes::{BufMut, BytesMut};
use chrono::NaiveDateTime;
use chrono::{DateTime, Utc};
use rand::{thread_rng, Rng};
use sha2::{Digest, Sha256};
use tokio::io::AsyncReadExt;
use tokio::net::tcp::OwnedReadHalf;
use tokio::net::tcp::OwnedWriteHalf;

use std::convert::TryInto;
use std::fmt;
use std::io::Write;
use std::net::{IpAddr::*, Ipv6Addr};
use std::{io, net::IpAddr, net::SocketAddr};
use tokio::net::TcpStream;

const MAGIC: [u8; 4] = [0xfa, 0x1a, 0xf9, 0xbf];

#[derive(Debug)]
pub struct MessageHeader {
    magic: [u8; 4],
    command: [u8; 12],
    pub body_length: u32,
    checksum: u32,
}

impl MessageHeader {
    pub fn new(command: [u8; 12], body_length: u32, checksum: u32) -> Self {
        MessageHeader {
            magic: MAGIC,
            command,
            body_length,
            checksum,
        }
    }

    pub async fn write_to_stream(&self, mut stream: &mut TcpStream) -> io::Result<()> {
        let mut header_buf = vec![];

        header_buf.write_all(&self.magic)?;
        header_buf.write_all(&self.command)?;
        header_buf.write_all(&u32::to_le_bytes(self.body_length))?;
        header_buf.write_all(&u32::to_le_bytes(self.checksum))?;

        tokio::io::AsyncWriteExt::write_all(&mut stream, &header_buf).await?;

        Ok(())
    }

    pub async fn read_from_stream(stream: &mut TcpStream) -> io::Result<Self> {
        Ok(Self {
            magic: read_4_bytes(stream).await?,
            command: read_12_bytes(stream).await?,
            body_length: u32::from_le_bytes(read_4_bytes(stream).await?),
            checksum: u32::from_le_bytes(read_4_bytes(stream).await?),
        })
    }
}

async fn read_4_bytes(stream: &mut TcpStream) -> io::Result<([u8; 4])> {
    let mut bytes = [0u8; 4];
    stream.read_exact(&mut bytes).await?;
    Ok(bytes)
}

async fn read_12_bytes(stream: &mut TcpStream) -> io::Result<([u8; 12])> {
    let mut bytes = [0u8; 12];
    stream.read_exact(&mut bytes).await?;
    Ok(bytes)
}

pub enum Message {
    Version(Version),
    Verack,
}

impl Message {
    pub async fn write_to_stream(&self, stream: &mut TcpStream) -> io::Result<()> {
        // Buffer for the message payload.
        let mut buffer = vec![];

        let header = match self {
            Self::Version(version) => version.encode(&mut buffer),
            Self::Verack => Ok(MessageHeader::new(
                *b"verack\0\0\0\0\0\0",
                0,
                checksum(&buffer),
            )),
            _ => unimplemented!(),
        }?;

        header.write_to_stream(stream).await?;
        tokio::io::AsyncWriteExt::write_all(stream, &buffer).await?;

        Ok(())
    }

    pub async fn read_from_stream(stream: &mut TcpStream) -> io::Result<Self> {
        let header = MessageHeader::read_from_stream(stream).await?;

        let message = match &header.command {
            b"version\0\0\0\0\0" => Self::Version(Version::read_from_stream(stream).await?),
            b"verack\0\0\0\0\0\0" => Self::Verack,
            _ => unimplemented!(),
        };

        Ok(message)
    }
}

#[derive(Debug)]
pub struct Version {
    version: u32,
    services: u64,
    timestamp: DateTime<Utc>,
    addr_recv: (u64, SocketAddr),
    addr_from: (u64, SocketAddr),
    nonce: u64,
    user_agent: String,
    start_height: u32,
    relay: bool,
}

impl Version {
    pub fn new(addr_recv: SocketAddr, addr_from: SocketAddr) -> Self {
        let mut rng = thread_rng();
        Self {
            version: 170_013,
            services: 1,
            timestamp: Utc::now(),
            addr_recv: (1, addr_recv),
            addr_from: (1, addr_from),
            nonce: rng.gen(),
            user_agent: String::from(""),
            start_height: 0,
            relay: false,
        }
    }

    pub fn encode(&self, body_buf: &mut Vec<u8>) -> io::Result<MessageHeader> {
        // Composition:
        //
        // Header (24 bytes):
        //
        // - 4 bytes of Magic,
        // - 12 bytes of command (this is the message name),
        // - 4 bytes of body length,
        // - 4 bytes of checksum (0ed initially, then computed after the body has been
        // written),
        //
        // Body (85 + variable bytes):
        //
        // - 4 bytes for the version
        // - 8 bytes for the peer services
        // - 8 for timestamp
        // - 8 + 16 + 2 (26) for the address_recv
        // - 8 + 16 + 2 (26) for the address_from
        // - 8 for the nonce
        // - 1, 3, 5 or 9 for compact size (variable)
        // - user_agent (variable)
        // - 4 for start height
        // - 1 for relay

        // Write the body, size is unkown at this point.
        body_buf.write_all(&u32::to_le_bytes(self.version))?;
        body_buf.write_all(&u64::to_le_bytes(self.services))?;
        body_buf.write_all(&i64::to_le_bytes(self.timestamp.timestamp()))?;

        write_addr(body_buf, self.addr_recv)?;
        write_addr(body_buf, self.addr_from)?;

        body_buf.write_all(&u64::to_le_bytes(self.nonce))?;
        let len = write_string(body_buf, &self.user_agent)?;
        body_buf.write_all(&u32::to_le_bytes(self.start_height))?;
        body_buf.write_all(&[self.relay as u8])?;

        // Write the header.
        let checksum = checksum(&body_buf);

        Ok(MessageHeader {
            magic: MAGIC,
            command: *b"version\0\0\0\0\0",
            body_length: (85 + len) as u32,
            checksum,
        })
    }

    pub async fn read_from_stream(mut stream: &mut TcpStream) -> io::Result<Self> {
        let version = stream.read_u32_le().await?;
        let services = stream.read_u64_le().await?;
        let timestamp = stream.read_i64_le().await?;
        let dt = DateTime::<Utc>::from_utc(NaiveDateTime::from_timestamp(timestamp, 0), Utc);

        let addr_recv = decode_addr(&mut stream).await?;
        let addr_from = decode_addr(&mut stream).await?;

        let nonce = stream.read_u64_le().await?;
        let user_agent = decode_string(&mut stream).await?;

        let start_height = stream.read_u32_le().await?;
        let relay = stream.read_u8().await? != 0;

        Ok(Self {
            version,
            services,
            timestamp: dt,
            addr_recv,
            addr_from,
            nonce,
            user_agent,
            start_height,
            relay,
        })
    }
}

fn write_addr(buf: &mut Vec<u8>, (services, addr): (u64, SocketAddr)) -> io::Result<()> {
    buf.write_all(&u64::to_le_bytes(services))?;

    let (ip, port) = match addr {
        SocketAddr::V4(v4) => (v4.ip().to_ipv6_mapped(), v4.port()),
        SocketAddr::V6(v6) => (*v6.ip(), v6.port()),
    };

    buf.write_all(&ip.octets())?;
    buf.write_all(&u16::to_be_bytes(port))?;

    Ok(())
}

fn write_string(buf: &mut Vec<u8>, s: &str) -> io::Result<usize> {
    // Bitcoin "CompactSize" encoding.
    let l = s.len();
    let cs_len = match l {
        0x0000_0000..=0x0000_00fc => {
            buf.write_all(&[l as u8])?;
            1
        }
        0x0000_00fd..=0x0000_ffff => {
            buf.write_all(&[0xfdu8])?;
            buf.write_all(&u16::to_le_bytes(l as u16))?;
            3 // bytes written
        }
        0x0001_0000..=0xffff_ffff => {
            buf.write_all(&[0xfeu8])?;
            buf.write_all(&u32::to_le_bytes(l as u32))?;
            5
        }
        _ => {
            buf.write_all(&[0xffu8])?;
            buf.write_all(&u64::to_le_bytes(l as u64))?;
            9
        }
    };

    buf.write_all(s.as_bytes())?;

    Ok(l + cs_len)
}

async fn decode_addr(stream: &mut TcpStream) -> io::Result<(u64, SocketAddr)> {
    let services = stream.read_u64_le().await?;

    let mut octets = [0u8; 16];
    stream.read_exact(&mut octets).await?;
    let v6_addr = Ipv6Addr::from(octets);

    let ip_addr = match v6_addr.to_ipv4() {
        Some(v4_addr) => V4(v4_addr),
        None => V6(v6_addr),
    };

    let port_le = stream.read_u16_le().await?;
    let port = port_le.to_be();

    Ok((services, SocketAddr::new(ip_addr, port)))
}

async fn decode_string(stream: &mut TcpStream) -> io::Result<String> {
    let flag = stream.read_u8().await?;

    let len = match flag {
        len @ 0x00..=0xfc => len as u64,
        0xfd => stream.read_u16_le().await? as u64,
        0xfe => stream.read_u32_le().await? as u64,
        0xff => stream.read_u64_le().await? as u64,
    };

    let mut buf = vec![0u8; len as usize];
    stream.read_exact(&mut buf).await?;
    Ok(String::from_utf8(buf).expect("invalid utf-8"))
}

fn checksum(bytes: &[u8]) -> u32 {
    let sha2 = Sha256::digest(bytes);
    let sha2d = Sha256::digest(&sha2);

    let mut checksum = [0u8; 4];
    checksum[0..4].copy_from_slice(&sha2d[0..4]);

    u32::from_le_bytes(checksum)
}
