//! Traits for encoding and decoding network message types.

use bytes::{Buf, BufMut};

use super::VarInt;

use std::io;

/// A trait for unifying encoding and decoding.
pub trait Codec {
    /// Encodes the payload into the supplied buffer.
    fn encode<B: BufMut>(&self, buffer: &mut B) -> io::Result<()>;

    /// Decodes the bytes and returns the payload.
    fn decode<B: Buf>(bytes: &mut B) -> io::Result<Self>
    where
        Self: Sized;
}

impl<T: Codec> Codec for Vec<T> {
    fn encode<B: BufMut>(&self, buffer: &mut B) -> io::Result<()> {
        VarInt(self.len()).encode(buffer)?;
        for element in self {
            element.encode(buffer)?;
        }

        Ok(())
    }

    fn decode<B: Buf>(bytes: &mut B) -> io::Result<Self>
    where
        Self: Sized,
    {
        let length = *VarInt::decode(bytes)?;
        (0..length)
            .map(|_| T::decode(bytes))
            .collect::<io::Result<Self>>()
    }
}
