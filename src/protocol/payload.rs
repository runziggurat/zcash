use chrono::{DateTime, NaiveDateTime, Utc};
use rand::{thread_rng, Rng};

use std::{
    io::{self, Cursor, Read, Write},
    net::{IpAddr::*, Ipv6Addr, SocketAddr},
};

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

    pub fn encode(&self, buffer: &mut Vec<u8>) -> io::Result<()> {
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
        buffer.write_all(&self.version.to_le_bytes())?;
        buffer.write_all(&self.services.to_le_bytes())?;
        buffer.write_all(&self.timestamp.timestamp().to_le_bytes())?;

        write_addr(buffer, self.addr_recv)?;
        write_addr(buffer, self.addr_from)?;

        buffer.write_all(&self.nonce.to_le_bytes())?;
        write_string(buffer, &self.user_agent)?;
        buffer.write_all(&self.start_height.to_le_bytes())?;
        buffer.write_all(&[self.relay as u8])?;

        Ok(())
    }

    pub fn decode(bytes: &mut Cursor<&[u8]>) -> io::Result<Self> {
        let version = u32::from_le_bytes(read_n_bytes(bytes)?);
        let services = u64::from_le_bytes(read_n_bytes(bytes)?);
        let timestamp = i64::from_le_bytes(read_n_bytes(bytes)?);
        let dt = DateTime::<Utc>::from_utc(NaiveDateTime::from_timestamp(timestamp, 0), Utc);

        let addr_recv = decode_addr(bytes)?;
        let addr_from = decode_addr(bytes)?;

        let nonce = u64::from_le_bytes(read_n_bytes(bytes)?);
        let user_agent = decode_string(bytes)?;

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

pub struct Nonce(u64);

impl Nonce {
    pub fn encode(&self, buffer: &mut Vec<u8>) -> io::Result<()> {
        buffer.write_all(&self.0.to_le_bytes())?;

        Ok(())
    }

    pub fn decode(bytes: &mut Cursor<&[u8]>) -> io::Result<Self> {
        let nonce = u64::from_le_bytes(read_n_bytes(bytes)?);

        Ok(Self(nonce))
    }
}

fn write_addr(buffer: &mut Vec<u8>, (services, addr): (u64, SocketAddr)) -> io::Result<()> {
    buffer.write_all(&services.to_le_bytes())?;

    let (ip, port) = match addr {
        SocketAddr::V4(v4) => (v4.ip().to_ipv6_mapped(), v4.port()),
        SocketAddr::V6(v6) => (*v6.ip(), v6.port()),
    };

    buffer.write_all(&ip.octets())?;
    buffer.write_all(&port.to_be_bytes())?;

    Ok(())
}

fn write_string(buffer: &mut Vec<u8>, s: &str) -> io::Result<usize> {
    // Bitcoin "CompactSize" encoding.
    let l = s.len();
    let cs_len = match l {
        0x0000_0000..=0x0000_00fc => {
            buffer.write_all(&[l as u8])?;
            1 // bytes written
        }
        0x0000_00fd..=0x0000_ffff => {
            buffer.write_all(&[0xfdu8])?;
            buffer.write_all(&(l as u16).to_le_bytes())?;
            3
        }
        0x0001_0000..=0xffff_ffff => {
            buffer.write_all(&[0xfeu8])?;
            buffer.write_all(&(l as u32).to_le_bytes())?;
            5
        }
        _ => {
            buffer.write_all(&[0xffu8])?;
            buffer.write_all(&(l as u64).to_le_bytes())?;
            9
        }
    };

    buffer.write_all(s.as_bytes())?;

    Ok(l + cs_len)
}

fn decode_addr(bytes: &mut Cursor<&[u8]>) -> io::Result<(u64, SocketAddr)> {
    let services = u64::from_le_bytes(read_n_bytes(bytes)?);

    let mut octets = [0u8; 16];
    bytes.read_exact(&mut octets)?;
    let v6_addr = Ipv6Addr::from(octets);

    let ip_addr = match v6_addr.to_ipv4() {
        Some(v4_addr) => V4(v4_addr),
        None => V6(v6_addr),
    };

    let port = u16::from_be_bytes(read_n_bytes(bytes)?);

    Ok((services, SocketAddr::new(ip_addr, port)))
}

fn decode_string(bytes: &mut Cursor<&[u8]>) -> io::Result<String> {
    let flag = u8::from_le_bytes(read_n_bytes(bytes)?);

    let len = match flag {
        len @ 0x00..=0xfc => len as u64,
        0xfd => u16::from_le_bytes(read_n_bytes(bytes)?) as u64,
        0xfe => u32::from_le_bytes(read_n_bytes(bytes)?) as u64,
        0xff => u64::from_le_bytes(read_n_bytes(bytes)?) as u64,
    };

    let mut buffer = vec![0u8; len as usize];
    bytes.read_exact(&mut buffer)?;
    Ok(String::from_utf8(buffer).expect("invalid utf-8"))
}

fn read_n_bytes<const N: usize>(bytes: &mut Cursor<&[u8]>) -> io::Result<[u8; N]> {
    let mut buffer = [0u8; N];
    bytes.read_exact(&mut buffer)?;

    Ok(buffer)
}
