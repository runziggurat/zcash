use super::VarInt;

pub trait Codec {
    fn encode(&self, buffer: &mut Vec<u8>) -> std::io::Result<()>;
    fn decode(bytes: &mut std::io::Cursor<&[u8]>) -> std::io::Result<Self>
    where
        Self: Sized;
}

impl<T: Codec> Codec for Vec<T> {
    fn encode(&self, buffer: &mut Vec<u8>) -> std::io::Result<()> {
        VarInt(self.len()).encode(buffer)?;
        for element in self {
            element.encode(buffer)?;
        }

        Ok(())
    }

    fn decode(bytes: &mut std::io::Cursor<&[u8]>) -> std::io::Result<Self>
    where
        Self: Sized,
    {
        let length = *VarInt::decode(bytes)?;
        (0..length)
            .map(|_| T::decode(bytes))
            .collect::<std::io::Result<Self>>()
    }
}
