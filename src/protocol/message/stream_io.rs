use crate::protocol::{
    message::{constants::*, Message, MessageHeader},
    payload::{
        block::{Block, Headers, LocatorHashes},
        codec::Codec,
        Addr, Inv, Nonce, Reject, Tx, Version,
    },
};

use tokio::io::{self, AsyncReadExt, AsyncWriteExt};

use std::io::Cursor;

impl MessageHeader {
    /// Writes the message header to the stream.
    pub async fn write_to_stream<T: AsyncWriteExt + Unpin>(
        &self,
        stream: &mut T,
    ) -> io::Result<()> {
        let mut buffer = Vec::with_capacity(24);
        self.encode(&mut buffer)?;

        stream.write_all(&buffer).await?;

        Ok(())
    }

    /// Reads a message header from the stream.
    pub async fn read_from_stream<T: AsyncReadExt + Unpin>(stream: &mut T) -> io::Result<Self> {
        let mut header: MessageHeader = Default::default();

        stream.read_exact(&mut header.magic).await?;
        stream.read_exact(&mut header.command).await?;
        header.body_length = stream.read_u32_le().await?;
        header.checksum = stream.read_u32_le().await?;

        Ok(header)
    }
}

impl Message {
    /// Writes the message to the stream.
    pub async fn write_to_stream<T: AsyncWriteExt + Unpin>(
        &self,
        stream: &mut T,
    ) -> io::Result<()> {
        // Buffer for the message payload.
        let mut buffer = vec![];
        let header = self.encode(&mut buffer)?;

        header.write_to_stream(stream).await?;
        stream.write_all(&buffer).await?;

        Ok(())
    }

    /// Reads a message from the stream.
    pub async fn read_from_stream<T: AsyncReadExt + Unpin>(stream: &mut T) -> io::Result<Self> {
        let header = MessageHeader::read_from_stream(stream).await?;

        let mut buffer = vec![0u8; header.body_length as usize];
        stream
            .read_exact(&mut buffer[..header.body_length as usize])
            .await?;

        let mut bytes = Cursor::new(&buffer[..]);

        let message = match header.command {
            VERSION_COMMAND => Self::Version(Version::decode(&mut bytes)?),
            VERACK_COMMAND => Self::Verack,
            PING_COMMAND => Self::Ping(Nonce::decode(&mut bytes)?),
            PONG_COMMAND => Self::Pong(Nonce::decode(&mut bytes)?),
            GETADDR_COMMAND => Self::GetAddr,
            ADDR_COMMAND => Self::Addr(Addr::decode(&mut bytes)?),
            GETHEADERS_COMMAND => Self::GetHeaders(LocatorHashes::decode(&mut bytes)?),
            HEADERS_COMMAND => Self::Headers(Headers::decode(&mut bytes)?),
            GETBLOCKS_COMMAND => Self::GetBlocks(LocatorHashes::decode(&mut bytes)?),
            BLOCK_COMMAND => Self::Block(Box::new(Block::decode(&mut bytes)?)),
            GETDATA_COMMAND => Self::GetData(Inv::decode(&mut bytes)?),
            INV_COMMAND => Self::Inv(Inv::decode(&mut bytes)?),
            NOTFOUND_COMMAND => Self::NotFound(Inv::decode(&mut bytes)?),
            MEMPOOL_COMMAND => Self::MemPool,
            TX_COMMAND => Self::Tx(Tx::decode(&mut bytes)?),
            REJECT_COMMAND => Self::Reject(Reject::decode(&mut bytes)?),
            _ => unimplemented!(),
        };

        Ok(message)
    }
}
