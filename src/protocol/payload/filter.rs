use std::io::{self, Cursor, ErrorKind, Read, Write};

use crate::protocol::payload::{codec::Codec, read_n_bytes};

#[derive(Debug, PartialEq, Default)]
pub struct FilterAdd {
    data: Vec<u8>,
}

#[derive(Debug, PartialEq, Default)]
pub struct FilterLoad {
    filter: Vec<u8>,
    hash_fn_count: u32,
    tweak: u32,
    flags: u8,
}

impl Codec for FilterAdd {
    fn encode(&self, buffer: &mut Vec<u8>) -> io::Result<()> {
        buffer.write_all(&self.data)
    }

    fn decode(bytes: &mut Cursor<&[u8]>) -> io::Result<Self>
    where
        Self: Sized,
    {
        let mut data = Vec::new();
        bytes.read_to_end(&mut data)?;

        if data.len() > 520 {
            return Err(io::Error::new(
                ErrorKind::InvalidData,
                format!(
                    "Maximum FilterAdd data length is 520, but got {}",
                    data.len()
                ),
            ));
        }

        Ok(Self { data })
    }
}

impl Codec for FilterLoad {
    fn encode(&self, buffer: &mut Vec<u8>) -> io::Result<()> {
        buffer.write_all(&self.filter)?;
        buffer.write_all(&self.hash_fn_count.to_le_bytes())?;
        buffer.write_all(&self.tweak.to_le_bytes())?;
        buffer.write_all(&[self.flags])
    }

    fn decode(bytes: &mut Cursor<&[u8]>) -> io::Result<Self>
    where
        Self: Sized,
    {
        // Have to read to end in order to get size of filter.
        // (we only know the final 9 bytes are reserved for the other fields)
        let mut buffer = Vec::new();
        let bytes_read = bytes.read_to_end(&mut buffer)?;

        const NON_FILTER_BYTES: usize = 4 + 4 + 1;
        if bytes_read < NON_FILTER_BYTES {
            return Err(io::Error::new(
                ErrorKind::InvalidData,
                format!(
                    "Minimum FilterLoad bytes required is {} but only got {}",
                    NON_FILTER_BYTES, bytes_read
                ),
            ));
        }
        let filter_bytes = bytes_read - NON_FILTER_BYTES;
        // maximum filter size is 36k bytes
        const MAX_FILTER_BYTES: usize = 36_000;
        if filter_bytes > MAX_FILTER_BYTES {
            return Err(io::Error::new(
                ErrorKind::InvalidData,
                format!(
                    "Maximum filter bytes is {} but got {}",
                    MAX_FILTER_BYTES, filter_bytes
                ),
            ));
        }

        let mut cursor = Cursor::new(&buffer[..]);

        let mut filter = vec![0; filter_bytes];
        cursor.read_exact(&mut filter)?;

        let hash_fn_count = u32::from_le_bytes(read_n_bytes(&mut cursor)?);
        let tweak = u32::from_le_bytes(read_n_bytes(&mut cursor)?);
        let flags = u8::from_le_bytes(read_n_bytes(&mut cursor)?);

        Ok(Self {
            filter,
            hash_fn_count,
            tweak,
            flags,
        })
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    #[ignore]
    fn filter_load_roundtrip() {
        let original = FilterLoad::default();

        let mut buffer = Vec::new();
        original.encode(&mut buffer).unwrap();

        let mut cursor = Cursor::new(&buffer[..]);
        let decoded = FilterLoad::decode(&mut cursor).unwrap();
        assert_eq!(decoded, original);
    }

    #[test]
    #[ignore]
    fn filter_add_roundtrip() {
        let original = FilterAdd::default();

        let mut buffer = Vec::new();
        original.encode(&mut buffer).unwrap();

        let mut cursor = Cursor::new(&buffer[..]);
        let decoded = FilterAdd::decode(&mut cursor).unwrap();
        assert_eq!(decoded, original);
    }
}
