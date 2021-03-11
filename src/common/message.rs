use byteorder::{BigEndian, ByteOrder, LittleEndian, WriteBytesExt};
use bytes::{BufMut, BytesMut};
use chrono::NaiveDateTime;
use chrono::{DateTime, Utc};
use rand::{thread_rng, Rng};
use sha2::{Digest, Sha256};
use tokio::io::AsyncReadExt;
use tokio::net::tcp::OwnedReadHalf;
use tokio::net::tcp::OwnedWriteHalf;

use std::convert::TryInto;
use std::fmt;
use std::net::{IpAddr::*, Ipv6Addr};
use std::{io, io::Write, net::IpAddr, net::SocketAddr};
use tokio::net::TcpStream;

#[derive(Debug)]
pub struct MessageHeader {
    magic: [u8; 4],
    command: [u8; 12],
    pub body_length: u32,
    checksum: u32,
}

impl MessageHeader {
    pub fn from(bytes: [u8; 24]) -> Self {
        Self {
            magic: bytes[..4].try_into().unwrap(),
            command: bytes[4..16].try_into().unwrap(),
            body_length: u32::from_le_bytes(bytes[16..20].try_into().unwrap()),
            checksum: u32::from_le_bytes(bytes[20..24].try_into().unwrap()),
        }
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

    pub fn encode(&self, buffer: &mut BytesMut) -> io::Result<()> {
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
        // Body (53 + variable bytes):
        //
        // - 4 bytes for the version
        // - 8 bytes for the peer services
        // - 8 + 16 + 2 for the address_recv
        // - 8 + 16 + 2 for the address_from
        // - 8 for the nonce
        // - 1, 3, 5 or 9 for compact size (variable)
        // - user_agent (variable)
        // - 4 for start height
        // - 1 for relay

        //  // Write the header.
        //  let mut writer = buffer.writer();
        //  writer.write_all(&[0xfa, 0x1a, 0xf9, 0xbf])?;
        //  writer.write_all(b"version\0\0\0\0\0")?;

        //  // Zeroed body length and checksum to be mutated after the body has been written.
        //  writer.write_u32::<LittleEndian>(0)?;
        //  writer.write_u32::<LittleEndian>(0)?;

        //  // Write the body.
        //  writer.write_u32::<LittleEndian>(self.version)?;
        //  writer.write_u64::<LittleEndian>(self.services)?;

        //  // Assumes the address is ipv4.
        //  // TODO: extract and support V6.
        //  if let (services, SocketAddr::V4(v4)) = self.address_recv {
        //      writer.write_u64::<LittleEndian>(services)?;
        //      let v6 = v4.ip().to_ipv6_mapped();
        //      writer.write_all(&v6.octets())?;
        //      writer.write_u16::<BigEndian>(v4.port())?;
        //  } else {
        //      panic!("Address isn't ipv4");
        //  }

        //  if let (services, SocketAddr::V4(v4)) = self.address_from {
        //      writer.write_u64::<LittleEndian>(services)?;
        //      let v6 = v4.ip().to_ipv6_mapped();
        //      writer.write_all(&v6.octets())?;
        //      writer.write_u16::<BigEndian>(v4.port())?;
        //  } else {
        //      panic!("Address isn't ipv4");
        //  }

        //  writer.write_u64::<LittleEndian>(self.nonce)?;

        //  // Bitcoin "CompactSize" encoding.
        //  let l = self.user_agent.len();
        //  let mut cs_len: u32 = l as u32;
        //  match l {
        //      0x0000_0000..=0x0000_00fc => writer.write_u8(l as u8)?,
        //      0x0000_00fd..=0x0000_ffff => {
        //          writer.write_u8(0xfd)?;
        //          writer.write_u16::<LittleEndian>(l as u16)?;
        //          cs_len += 3;
        //      }
        //      0x0001_0000..=0xffff_ffff => {
        //          writer.write_u8(0xfe)?;
        //          writer.write_u32::<LittleEndian>(l as u32)?;
        //          cs_len += 5;
        //      }
        //      _ => {
        //          writer.write_u8(0xff)?;
        //          writer.write_u64::<LittleEndian>(l as u64)?;
        //          cs_len += 9;
        //      }
        //  }

        //  writer.write_all(self.user_agent.as_bytes())?;

        //  writer.write_u32::<LittleEndian>(self.start_height)?;
        //  writer.write_u8(self.relay as u8)?;

        //  // Set the length in the previously zeroed portion of the header.
        //  let body_len = 53 + cs_len;
        //  let mut buf = [0u8; 4];
        //  LittleEndian::write_u32(&mut buf, body_len);
        //  buffer[16..][..4].copy_from_slice(&buf);

        //  // Compute the 4 byte checksum and replace it in the previously zeroed portion of the
        //  // header.
        //  let checksum = checksum(&buffer[24..]);
        //  buffer[20..][..4].copy_from_slice(&checksum);

        Ok(())
    }

    pub async fn decode(mut stream: &mut TcpStream) -> io::Result<Self> {
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
    stream.read_exact(&mut buf).await;
    Ok(String::from_utf8(buf).expect("invalid utf-8"))
}

fn checksum(bytes: &[u8]) -> [u8; 4] {
    let sha2 = Sha256::digest(bytes);
    let sha2d = Sha256::digest(&sha2);

    let mut checksum = [0u8; 4];
    checksum[0..4].copy_from_slice(&sha2d[0..4]);

    checksum
}
