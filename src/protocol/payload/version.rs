//! Version payload types.

use crate::protocol::payload::{
    addr::NetworkAddr, codec::Codec, read_n_bytes, read_timestamp, Nonce, ProtocolVersion, VarStr,
};

use bytes::{Buf, BufMut};
use chrono::{DateTime, Utc};

use std::{io, net::SocketAddr};

/// A version payload.
#[derive(Debug, PartialEq, Clone)]
pub struct Version {
    /// The protocol version of the sender.
    pub version: ProtocolVersion,
    /// The services supported by the sender.
    pub services: u64,
    /// The timestamp of the message.
    pub timestamp: DateTime<Utc>,
    /// The receiving address of the message.
    pub addr_recv: NetworkAddr,
    /// The sender of the message.
    pub addr_from: NetworkAddr,
    /// The nonce associated with this message.
    pub nonce: Nonce,
    /// The user agent of the sender.
    pub user_agent: VarStr,
    /// The start last block received by the sender.
    pub start_height: u32,
    /// Specifies if the receiver should relay transactions.
    pub relay: bool,
}

impl Version {
    /// Constructs a `Version`, where `addr_recv` is the remote `zcashd`/`zebra` node address and
    /// `addr_from` is our local node address.
    pub fn new(addr_recv: SocketAddr, addr_from: SocketAddr) -> Self {
        Self {
            version: ProtocolVersion::current(),
            services: 1,
            timestamp: Utc::now(),
            addr_recv: NetworkAddr {
                last_seen: None,
                services: 1,
                addr: addr_recv,
            },
            addr_from: NetworkAddr {
                last_seen: None,
                services: 1,
                addr: addr_from,
            },
            nonce: Nonce::default(),
            user_agent: VarStr(String::from("")),
            start_height: 0,
            relay: false,
        }
    }

    /// Sets the protocol version.
    pub fn with_version(mut self, version: u32) -> Self {
        self.version = ProtocolVersion(version);
        self
    }
}

impl Codec for Version {
    fn encode<B: BufMut>(&self, buffer: &mut B) -> io::Result<()> {
        self.version.encode(buffer)?;
        buffer.put_u64_le(self.services);
        buffer.put_i64_le(self.timestamp.timestamp());

        self.addr_recv.encode_without_timestamp(buffer)?;
        self.addr_from.encode_without_timestamp(buffer)?;

        self.nonce.encode(buffer)?;
        self.user_agent.encode(buffer)?;
        buffer.put_u32_le(self.start_height);
        buffer.put_u8(self.relay as u8);

        Ok(())
    }

    fn decode<B: Buf>(bytes: &mut B) -> io::Result<Self> {
        let version = ProtocolVersion::decode(bytes)?;
        let services = u64::from_le_bytes(read_n_bytes(bytes)?);
        let timestamp = read_timestamp(bytes)?;

        let addr_recv = NetworkAddr::decode_without_timestamp(bytes)?;
        let addr_from = NetworkAddr::decode_without_timestamp(bytes)?;

        let nonce = Nonce::decode(bytes)?;
        let user_agent = VarStr::decode(bytes)?;

        let start_height = u32::from_le_bytes(read_n_bytes(bytes)?);
        let relay = u8::from_le_bytes(read_n_bytes(bytes)?) != 0;

        Ok(Self {
            version,
            services,
            timestamp,
            addr_recv,
            addr_from,
            nonce,
            user_agent,
            start_height,
            relay,
        })
    }
}
