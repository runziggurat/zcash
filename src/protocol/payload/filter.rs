//! Bloom filtering types, see [BIP 37](https://github.com/bitcoin/bips/blob/master/bip-0037.mediawiki).

use std::io::{self, Cursor, ErrorKind, Read};

use bytes::{Buf, BufMut};

use crate::protocol::payload::{codec::Codec, read_n_bytes};

/// A modification to an existing filter.
#[derive(Debug, PartialEq, Eq, Default, Clone)]
pub struct FilterAdd {
    /// The data element to add to the current filter.
    pub data: Vec<u8>,
}

/// A new filter on the connection.
#[derive(Debug, PartialEq, Eq, Default, Clone)]
pub struct FilterLoad {
    /// The filter itself.
    pub filter: Vec<u8>,
    /// The number of hash functions to use in this filter.
    pub hash_fn_count: u32,
    /// A random value to add to the hash function's seed.
    pub tweak: u32,
    /// Flags that control how matched items are added to the filter.
    pub flags: u8,
}

impl Codec for FilterAdd {
    fn encode<B: BufMut>(&self, buffer: &mut B) -> io::Result<()> {
        buffer.put_slice(&self.data);

        Ok(())
    }

    fn decode<B: Buf>(bytes: &mut B) -> io::Result<Self>
    where
        Self: Sized,
    {
        let mut data = Vec::new();
        bytes.reader().read_to_end(&mut data)?;

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
    fn encode<B: BufMut>(&self, buffer: &mut B) -> io::Result<()> {
        buffer.put_slice(&self.filter);
        buffer.put_u32_le(self.hash_fn_count);
        buffer.put_u32_le(self.tweak);
        buffer.put_u8(self.flags);

        Ok(())
    }

    fn decode<B: Buf>(bytes: &mut B) -> io::Result<Self>
    where
        Self: Sized,
    {
        // Have to read to end in order to get size of filter.
        // (we only know the final 9 bytes are reserved for the other fields)
        let mut buffer = Vec::new();
        let bytes_read = bytes.reader().read_to_end(&mut buffer)?;

        const NON_FILTER_BYTES: usize = 4 + 4 + 1;
        if bytes_read < NON_FILTER_BYTES {
            return Err(io::Error::new(
                ErrorKind::InvalidData,
                format!(
                    "Minimum FilterLoad bytes required is {NON_FILTER_BYTES} but only got {bytes_read}"
                ),
            ));
        }
        let filter_bytes = bytes_read - NON_FILTER_BYTES;
        // maximum filter size is 36k bytes
        const MAX_FILTER_BYTES: usize = 36_000;
        if filter_bytes > MAX_FILTER_BYTES {
            return Err(io::Error::new(
                ErrorKind::InvalidData,
                format!("Maximum filter bytes is {MAX_FILTER_BYTES} but got {filter_bytes}"),
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
