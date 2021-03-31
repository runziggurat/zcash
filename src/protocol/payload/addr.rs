use crate::protocol::payload::{read_n_bytes, VarInt};

use chrono::{DateTime, NaiveDateTime, Utc};

use std::{
    io::{self, Cursor, Read, Write},
    net::{IpAddr::*, Ipv6Addr, SocketAddr},
};

pub struct Addr {
    count: VarInt,
    addrs: Vec<NetworkAddr>,
}

impl Addr {
    pub fn encode(&self, buffer: &mut Vec<u8>) -> io::Result<()> {
        self.count.encode(buffer)?;

        for addr in &self.addrs {
            addr.encode(buffer)?;
        }

        Ok(())
    }

    pub fn decode(bytes: &mut Cursor<&[u8]>) -> io::Result<Self> {
        let count = VarInt::decode(bytes)?;
        let mut addrs = vec![];

        for _ in 0..count.0 {
            addrs.push(NetworkAddr::decode(bytes)?)
        }

        Ok(Self { count, addrs })
    }
}

struct NetworkAddr {
    // Node: Present only when version is >= 31402
    last_seen: Option<DateTime<Utc>>,
    services: u64,
    addr: SocketAddr,
}

impl NetworkAddr {
    fn encode(&self, buffer: &mut Vec<u8>) -> io::Result<()> {
        buffer.write_all(
            &self
                .last_seen
                .expect("missing timestamp")
                .timestamp()
                .to_le_bytes(),
        )?;

        self.encode_without_timestamp(buffer)?;

        Ok(())
    }

    fn encode_without_timestamp(&self, buffer: &mut Vec<u8>) -> io::Result<()> {
        buffer.write_all(&self.services.to_le_bytes())?;

        let (ip, port) = match self.addr {
            SocketAddr::V4(v4) => (v4.ip().to_ipv6_mapped(), v4.port()),
            SocketAddr::V6(v6) => (*v6.ip(), v6.port()),
        };

        buffer.write_all(&ip.octets())?;
        buffer.write_all(&port.to_be_bytes())?;

        Ok(())
    }

    fn decode(bytes: &mut Cursor<&[u8]>) -> io::Result<Self> {
        let timestamp = i64::from_le_bytes(read_n_bytes(bytes)?);
        let dt = DateTime::<Utc>::from_utc(NaiveDateTime::from_timestamp(timestamp, 0), Utc);

        let without_timestamp = NetworkAddr::decode_without_timestamp(bytes)?;

        Ok(Self {
            last_seen: Some(dt),
            ..without_timestamp
        })
    }

    fn decode_without_timestamp(bytes: &mut Cursor<&[u8]>) -> io::Result<Self> {
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
