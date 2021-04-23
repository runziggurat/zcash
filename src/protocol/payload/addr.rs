use crate::protocol::payload::{read_n_bytes, VarInt};

use std::convert::TryInto;

use chrono::{DateTime, NaiveDateTime, Utc};

use std::{
    io::{self, Cursor, Read, Write},
    net::{IpAddr::*, Ipv6Addr, SocketAddr},
};

#[derive(Debug)]
pub struct Addr {
    count: VarInt,
    addrs: Vec<NetworkAddr>,
}

impl Addr {
    pub fn empty() -> Self {
        Self {
            count: VarInt(0),
            addrs: Vec::new(),
        }
    }

    pub fn new(addrs: Vec<NetworkAddr>) -> Self {
        let count = VarInt(addrs.len());
        Addr { count, addrs }
    }

    pub fn encode(&self, buffer: &mut Vec<u8>) -> io::Result<()> {
        self.count.encode(buffer)?;

        for addr in &self.addrs {
            addr.encode(buffer)?;
        }

        Ok(())
    }

    pub fn decode(bytes: &mut Cursor<&[u8]>) -> io::Result<Self> {
        let count = VarInt::decode(bytes)?;
        let mut addrs = Vec::with_capacity(count.0);

        for _ in 0..count.0 {
            let addr = NetworkAddr::decode(bytes)?;
            addrs.push(addr);
        }

        Ok(Self { count, addrs })
    }

    /// Returns an iterator over its list of [NetworkAddr]'s
    pub fn iter(&self) -> std::slice::Iter<NetworkAddr> {
        self.addrs.iter()
    }
}

#[derive(Debug, Clone)]
pub struct NetworkAddr {
    // Note: Present only when version is >= 31402
    pub(super) last_seen: Option<DateTime<Utc>>,
    pub(super) services: u64,
    pub addr: SocketAddr,
}

impl NetworkAddr {
    /// Creates a new NetworkAddr with the given socket address, `last_seen=chrono::Utc::now()`,
    /// and `services=1` (only NODE_NETWORK is enabled)
    pub fn new(addr: SocketAddr) -> Self {
        Self {
            last_seen: Some(chrono::Utc::now()),
            services: 1,
            addr,
        }
    }

    pub(super) fn encode(&self, buffer: &mut Vec<u8>) -> io::Result<()> {
        let timestamp: u32 = self
            .last_seen
            .expect("missing timestamp")
            .timestamp()
            .try_into()
            .unwrap();
        buffer.write_all(&timestamp.to_le_bytes())?;

        self.encode_without_timestamp(buffer)?;

        Ok(())
    }

    pub(super) fn encode_without_timestamp(&self, buffer: &mut Vec<u8>) -> io::Result<()> {
        buffer.write_all(&self.services.to_le_bytes())?;

        let (ip, port) = match self.addr {
            SocketAddr::V4(v4) => (v4.ip().to_ipv6_mapped(), v4.port()),
            SocketAddr::V6(v6) => (*v6.ip(), v6.port()),
        };

        buffer.write_all(&ip.octets())?;
        buffer.write_all(&port.to_be_bytes())?;

        Ok(())
    }

    pub(super) fn decode(bytes: &mut Cursor<&[u8]>) -> io::Result<Self> {
        let timestamp = u32::from_le_bytes(read_n_bytes(bytes)?);
        let dt = DateTime::<Utc>::from_utc(NaiveDateTime::from_timestamp(timestamp.into(), 0), Utc);

        let without_timestamp = Self::decode_without_timestamp(bytes)?;

        Ok(Self {
            last_seen: Some(dt),
            ..without_timestamp
        })
    }

    pub(super) fn decode_without_timestamp(bytes: &mut Cursor<&[u8]>) -> io::Result<Self> {
        let services = u64::from_le_bytes(read_n_bytes(bytes)?);

        let mut octets = [0u8; 16];
        bytes.read_exact(&mut octets)?;
        let v6_addr = Ipv6Addr::from(octets);

        let ip_addr = match v6_addr.to_ipv4() {
            Some(v4_addr) => V4(v4_addr),
            None => V6(v6_addr),
        };

        let port = u16::from_be_bytes(read_n_bytes(bytes)?);

        Ok(Self {
            last_seen: None,
            services,
            addr: SocketAddr::new(ip_addr, port),
        })
    }
}
