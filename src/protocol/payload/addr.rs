//! Network address types.

use crate::protocol::payload::{codec::Codec, read_n_bytes, read_timestamp};

use std::convert::TryInto;

use bytes::{Buf, BufMut};
use time::OffsetDateTime;

use std::{
    io,
    net::{IpAddr::*, Ipv6Addr, SocketAddr},
};

/// A list of network addresses, used for peering.
#[derive(Debug, PartialEq, Clone)]
pub struct Addr {
    pub addrs: Vec<NetworkAddr>,
}

impl Addr {
    /// Returns an `Addr` with no addresses.
    pub fn empty() -> Self {
        Self { addrs: Vec::new() }
    }

    /// Returns an `Addr` with the given addresses.
    pub fn new(addrs: Vec<NetworkAddr>) -> Self {
        Addr { addrs }
    }

    /// Returns an iterator over the list of network addresses.
    pub fn iter(&self) -> std::slice::Iter<NetworkAddr> {
        self.addrs.iter()
    }
}

impl Codec for Addr {
    fn encode<B: BufMut>(&self, buffer: &mut B) -> io::Result<()> {
        self.addrs.encode(buffer)
    }

    fn decode<B: Buf>(bytes: &mut B) -> io::Result<Self> {
        Ok(Self::new(Vec::decode(bytes)?))
    }
}

/// A network address.
#[derive(Debug, Clone, PartialEq)]
pub struct NetworkAddr {
    /// The last time this address was seen.
    /// Note: Present only when version is >= 31402
    pub last_seen: Option<OffsetDateTime>,
    /// The services supported by this address.
    pub services: u64,
    /// The socket address.
    pub addr: SocketAddr,
}

impl NetworkAddr {
    /// Creates a new NetworkAddr with the given socket address, `last_seen=OffsetDateTime`,
    /// and `services=1` (only `NODE_NETWORK` is enabled).
    pub fn new(addr: SocketAddr) -> Self {
        Self {
            last_seen: Some(OffsetDateTime::now_utc()),
            services: 1,
            addr,
        }
    }

    pub fn encode_without_timestamp<B: BufMut>(&self, buffer: &mut B) -> io::Result<()> {
        buffer.put_u64_le(self.services);

        let (ip, port) = match self.addr {
            SocketAddr::V4(v4) => (v4.ip().to_ipv6_mapped(), v4.port()),
            SocketAddr::V6(v6) => (*v6.ip(), v6.port()),
        };

        buffer.put_slice(&ip.octets());
        buffer.put_u16(port);

        Ok(())
    }

    pub(super) fn decode_without_timestamp<B: Buf>(bytes: &mut B) -> io::Result<Self> {
        let services = u64::from_le_bytes(read_n_bytes(bytes)?);

        let mut octets = [0u8; 16];
        bytes.copy_to_slice(&mut octets);
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

impl Codec for NetworkAddr {
    fn encode<B: BufMut>(&self, buffer: &mut B) -> io::Result<()> {
        let timestamp: u32 = self
            .last_seen
            .expect("missing timestamp")
            .unix_timestamp()
            .try_into()
            .unwrap();
        buffer.put_u32_le(timestamp);

        self.encode_without_timestamp(buffer)?;

        Ok(())
    }

    fn decode<B: Buf>(bytes: &mut B) -> io::Result<Self> {
        let timestamp = read_timestamp(bytes)?;
        let without_timestamp = Self::decode_without_timestamp(bytes)?;

        Ok(Self {
            last_seen: Some(timestamp),
            ..without_timestamp
        })
    }
}
