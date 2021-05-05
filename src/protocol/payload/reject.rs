use crate::protocol::payload::{codec::Codec, read_n_bytes, VarStr};

use std::io::{self, Cursor, Read, Write};

#[derive(Debug)]
pub struct Reject {
    message: VarStr,
    pub ccode: CCode,
    reason: VarStr,
    // Optional extra data provided by some errors.
    // Currently, all errors which provide this field fill it with
    // the TXID or block header hash of the object being rejected,
    // so the field is 32 bytes.
    //
    // We support any length data to fully adhere to the spec.
    data: Vec<u8>,
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
const OBSELETE_CODE: u8 = 0x11;
const DUPLICATE_CODE: u8 = 0x12;
const NON_STANDARD_CODE: u8 = 0x40;
const DUST_CODE: u8 = 0x41;
const INSUFFICIENT_FEE_CODE: u8 = 0x42;
const CHECKPOINT_CODE: u8 = 0x43;
const OTHER_CODE: u8 = 0x50;

#[derive(Debug, Clone, Copy, PartialEq)]
pub enum CCode {
    Malformed,
    Invalid,
    Obselete,
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
            Self::Obselete => OBSELETE_CODE,
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
            OBSELETE_CODE => Ok(Self::Obselete),
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

impl CCode {
    pub fn is_obsolete(&self) -> bool {
        *self == Self::Obselete
    }

    pub fn is_invalid(&self) -> bool {
        *self == Self::Invalid
    }
}
