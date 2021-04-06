use crate::protocol::payload::{addr::NetworkAddr, read_n_bytes, Nonce, ProtocolVersion, VarStr};

use chrono::{DateTime, NaiveDateTime, Utc};

use std::{
    io::{self, Cursor, Write},
    net::SocketAddr,
};

#[derive(Debug)]
pub struct Version {
    version: ProtocolVersion,
    services: u64,
    timestamp: DateTime<Utc>,
    addr_recv: NetworkAddr,
    addr_from: NetworkAddr,
    nonce: Nonce,
    user_agent: VarStr,
    start_height: u32,
    relay: bool,
}

impl Version {
    pub fn new(addr_recv: SocketAddr, addr_from: SocketAddr) -> Self {
        Self {
            // TODO: make constants.
            version: ProtocolVersion(170_013),
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

    pub fn encode(&self, buffer: &mut Vec<u8>) -> io::Result<()> {
        self.version.encode(buffer)?;
        buffer.write_all(&self.services.to_le_bytes())?;
        buffer.write_all(&self.timestamp.timestamp().to_le_bytes())?;

        self.addr_recv.encode_without_timestamp(buffer)?;
        self.addr_from.encode_without_timestamp(buffer)?;

        self.nonce.encode(buffer)?;
        self.user_agent.encode(buffer)?;
        buffer.write_all(&self.start_height.to_le_bytes())?;
        buffer.write_all(&[self.relay as u8])?;

        Ok(())
    }

    pub fn decode(bytes: &mut Cursor<&[u8]>) -> io::Result<Self> {
        let version = ProtocolVersion::decode(bytes)?;
        let services = u64::from_le_bytes(read_n_bytes(bytes)?);
        let timestamp = i64::from_le_bytes(read_n_bytes(bytes)?);
        let dt = DateTime::<Utc>::from_utc(NaiveDateTime::from_timestamp(timestamp, 0), Utc);

        let addr_recv = NetworkAddr::decode_without_timestamp(bytes)?;
        let addr_from = NetworkAddr::decode_without_timestamp(bytes)?;

        let nonce = Nonce::decode(bytes)?;
        let user_agent = VarStr::decode(bytes)?;

        let start_height = u32::from_le_bytes(read_n_bytes(bytes)?);
        let relay = u8::from_le_bytes(read_n_bytes(bytes)?) != 0;

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
