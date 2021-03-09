use byteorder::BigEndian;
use byteorder::ByteOrder;
use byteorder::LittleEndian;
use byteorder::WriteBytesExt;
use bytes::BufMut;
use bytes::BytesMut;
use chrono::DateTime;
use chrono::Utc;
use sha2::Digest;
use sha2::Sha256;

use std::io;
use std::io::Write;
use std::net::SocketAddr;

pub struct Version {
    version: u32,
    services: u64,
    timestamp: DateTime<Utc>,
    address_recv: (u64, SocketAddr),
    address_from: (u64, SocketAddr),
    nonce: u64,
    user_agent: String,
    start_height: u32,
    relay: bool,
}

impl Version {
    fn encode(&self, buffer: &mut BytesMut) -> io::Result<()> {
        // Composition:
        //
        // Header:
        //
        // - 4 bytes of Magic,
        // - 12 bytes of command (this is the message name),
        // - 4 bytes of body length,
        // - 4 bytes of checksum (0ed initially, then computed after the body has been
        // written),
        //
        // Body:
        //
        // - 4 bytes for the version
        // - 8 bytes for the peer services
        // - 8 + 16 + 2 for the address_recv
        // - 8 + 16 + 2 for the address_from
        // - 8 for the nonce
        // - 1, 3, 5 or 9 for compact size (variable)
        // - user_agent, length is variable but can be returned from the write operation
        // - 4 for start height
        // - 1 for relay

        // Write the header.
        let mut writer = buffer.writer();
        writer.write_all(&[0xfa, 0x1a, 0xf9, 0xbf])?;
        writer.write_all(b"version\0\0\0\0\0")?;

        // Zeroed body length and checksum to be mutated after the body has been written.
        writer.write_u32::<LittleEndian>(0)?;
        writer.write_u32::<LittleEndian>(0)?;

        // Write the body.
        writer.write_u32::<LittleEndian>(self.version)?;
        writer.write_u64::<LittleEndian>(self.services)?;

        // Assumes the address is ipv6.
        // TODO: extract and support V4.
        if let (services, SocketAddr::V6(v6)) = self.address_recv {
            writer.write_u64::<LittleEndian>(services)?;
            writer.write_all(&v6.ip().octets())?;
            writer.write_u16::<BigEndian>(v6.port())?;
        } else {
            panic!("Address isn't ipv6");
        }

        if let (services, SocketAddr::V6(v6)) = self.address_from {
            writer.write_u64::<LittleEndian>(services)?;
            writer.write_all(&v6.ip().octets())?;
            writer.write_u16::<BigEndian>(v6.port())?;
        } else {
            panic!("Address isn't ipv6");
        }

        writer.write_u64::<LittleEndian>(self.nonce)?;

        // Bitcoin "CompactSize" encoding.
        let l = self.user_agent.len();
        let mut cs_len: u32 = l as u32;
        match l {
            0x0000_0000..=0x0000_00fc => writer.write_u8(l as u8)?,
            0x0000_00fd..=0x0000_ffff => {
                writer.write_u8(0xfd)?;
                writer.write_u16::<LittleEndian>(l as u16)?;
                cs_len += 3;
            }
            0x0001_0000..=0xffff_ffff => {
                writer.write_u8(0xfe)?;
                writer.write_u32::<LittleEndian>(l as u32)?;
                cs_len += 5;
            }
            _ => {
                writer.write_u8(0xff)?;
                writer.write_u64::<LittleEndian>(l as u64)?;
                cs_len += 9;
            }
        }

        writer.write_all(self.user_agent.as_bytes())?;

        writer.write_u32::<LittleEndian>(self.start_height)?;
        writer.write_u8(self.relay as u8)?;

        // Set the length in the previously zeroed portion of the header.
        let body_len = 53 + cs_len;
        let mut buf = [0u8; 4];
        LittleEndian::write_u32(&mut buf, body_len);
        buffer[16..][..4].copy_from_slice(&buf);

        // Compute the 4 byte checksum and replace it in the previously zeroed portion of the
        // header.
        let checksum = checksum(&buffer[24..]);
        buffer[20..][..4].copy_from_slice(&checksum);

        Ok(())
    }
}

fn checksum(bytes: &[u8]) -> [u8; 4] {
    let sha2 = Sha256::digest(bytes);
    let sha2d = Sha256::digest(&sha2);

    let mut checksum = [0u8; 4];
    checksum[0..4].copy_from_slice(&sha2d[0..4]);

    checksum
}
