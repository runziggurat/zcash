//! Reject payload types.

use crate::protocol::payload::{codec::Codec, read_n_bytes, VarStr};

use std::io::{self, Cursor, Read, Write};

/// A reject message payload.
#[derive(Debug, PartialEq, Clone)]
pub struct Reject {
    /// The type of message rejected.
    pub message: VarStr,
    /// The code of the reason for rejection.
    pub ccode: CCode,
    /// The reason.
    pub reason: VarStr,
    /// Optional extra data provided by some errors.
    /// Currently, all errors which provide this field fill it with
    /// the TXID or block header hash of the object being rejected,
    /// so the field is 32 bytes.
    ///
    /// We support any length data to fully adhere to the spec.
    pub data: Vec<u8>,
}

impl Codec for Reject {
    fn encode(&self, buffer: &mut Vec<u8>) -> io::Result<()> {
        self.message.encode(buffer)?;
        self.ccode.encode(buffer)?;
        self.reason.encode(buffer)?;
        buffer.write_all(&self.data)
    }

    fn decode(bytes: &mut Cursor<&[u8]>) -> io::Result<Self> {
        let message = VarStr::decode(bytes)?;
        let ccode = CCode::decode(bytes)?;
        let reason = VarStr::decode(bytes)?;

        // Current usage of the data field is `Option<[u8; 32]>`,
        // but the spec allows for any length [u8], so we support that case.
        let mut data = Vec::new();
        bytes.read_to_end(&mut data)?;

        Ok(Self {
            message,
            ccode,
            reason,
            data,
        })
    }
}

const MALFORMED_CODE: u8 = 0x01;
const INVALID_CODE: u8 = 0x10;
const OBSOLETE_CODE: u8 = 0x11;
const DUPLICATE_CODE: u8 = 0x12;
const NON_STANDARD_CODE: u8 = 0x40;
const DUST_CODE: u8 = 0x41;
const INSUFFICIENT_FEE_CODE: u8 = 0x42;
const CHECKPOINT_CODE: u8 = 0x43;
const OTHER_CODE: u8 = 0x50;

/// The code specifying the reject reason.
#[derive(Debug, Clone, Copy, PartialEq)]
pub enum CCode {
    Malformed,
    Invalid,
    Obsolete,
    Duplicate,
    NonStandard,
    Dust,
    InsufficientFee,
    Checkpoint,
    Other,
}

impl Codec for CCode {
    fn encode(&self, buffer: &mut Vec<u8>) -> io::Result<()> {
        let code: u8 = match self {
            Self::Malformed => MALFORMED_CODE,
            Self::Invalid => INVALID_CODE,
            Self::Obsolete => OBSOLETE_CODE,
            Self::Duplicate => DUPLICATE_CODE,
            Self::NonStandard => NON_STANDARD_CODE,
            Self::Dust => DUST_CODE,
            Self::InsufficientFee => INSUFFICIENT_FEE_CODE,
            Self::Checkpoint => CHECKPOINT_CODE,
            Self::Other => OTHER_CODE,
        };

        buffer.write_all(&[code])
    }

    fn decode(bytes: &mut Cursor<&[u8]>) -> io::Result<Self> {
        let code: [u8; 1] = read_n_bytes(bytes)?;

        match code[0] {
            MALFORMED_CODE => Ok(Self::Malformed),
            INVALID_CODE => Ok(Self::Invalid),
            OBSOLETE_CODE => Ok(Self::Obsolete),
            DUPLICATE_CODE => Ok(Self::Duplicate),
            NON_STANDARD_CODE => Ok(Self::NonStandard),
            DUST_CODE => Ok(Self::Dust),
            INSUFFICIENT_FEE_CODE => Ok(Self::InsufficientFee),
            CHECKPOINT_CODE => Ok(Self::Checkpoint),
            OTHER_CODE => Ok(Self::Other),
            _ => Err(io::Error::new(
                io::ErrorKind::InvalidData,
                format!("Invalid CCode {:#x}", code[0]),
            )),
        }
    }
}
