//! Transaction-related types.

use bytes::{Buf, BufMut};
use sha2::Digest;

use crate::protocol::payload::{codec::Codec, read_n_bytes, Hash, VarInt};

use std::{convert::TryInto, io};

use crate::protocol::payload::inv::{InvHash, ObjectKind};

/// A Zcash transaction ([spec](https://zips.z.cash/protocol/canopy.pdf#txnencodingandconsensus)).
///
/// Supports V1-V4, V5 isn't yet stable.
#[derive(Debug, PartialEq, Clone)]
pub enum Tx {
    V1(TxV1),
    V2(TxV2),
    V3(TxV3),
    V4(TxV4),
    // Not yet stabilised.
    V5,
}

impl Tx {
    /// Calculates the double Sha256 hash for this transaction.
    pub fn double_sha256(&self) -> io::Result<Hash> {
        let mut buffer = Vec::new();

        self.encode(&mut buffer)?;

        let hash_bytes_1 = sha2::Sha256::digest(&buffer);
        let hash_bytes_2 = sha2::Sha256::digest(&hash_bytes_1);

        let hash = Hash::new(hash_bytes_2.try_into().unwrap());

        Ok(hash)
    }

    /// Convenience function which creates the [`InvHash`] for this `Tx`.
    pub fn inv_hash(&self) -> InvHash {
        InvHash::new(ObjectKind::Tx, self.double_sha256().unwrap())
    }
}

impl Codec for Tx {
    fn encode<B: BufMut>(&self, buffer: &mut B) -> io::Result<()> {
        match self {
            Tx::V1(tx) => {
                // The overwintered flag is NOT set.
                buffer.put_u32_le(1u32);
                tx.encode(buffer)?;
            }
            Tx::V2(tx) => {
                // The overwintered flag is NOT set.
                buffer.put_u32_le(2u32);
                tx.encode(buffer)?;
            }
            Tx::V3(tx) => {
                // The overwintered flag IS set.
                buffer.put_u32_le(3u32 | 1 << 31);
                tx.encode(buffer)?;
            }
            Tx::V4(tx) => {
                // The overwintered flag IS set.
                buffer.put_u32_le(4u32 | 1 << 31);
                tx.encode(buffer)?;
            }
            Tx::V5 => unimplemented!(),
        }

        Ok(())
    }

    fn decode<B: Buf>(bytes: &mut B) -> io::Result<Self> {
        use std::io::{Error, ErrorKind};

        let (version, overwinter) = {
            const LOW_31_BITS: u32 = !(1 << 31);
            let header = u32::from_le_bytes(read_n_bytes(bytes)?);

            // Extract transaction version and check if overwinter flag is set.
            (header & LOW_31_BITS, header >> 31 != 0)
        };

        let tx = match (version, overwinter) {
            (1, false) => Self::V1(TxV1::decode(bytes)?),
            (2, false) => Self::V2(TxV2::decode(bytes)?),
            (3, true) => Self::V3(TxV3::decode(bytes)?),
            (4, true) => Self::V4(TxV4::decode(bytes)?),
            (5, true) => unimplemented!(),
            (version, overwinter) => {
                return Err(Error::new(
                    ErrorKind::InvalidData,
                    format!("Couldn't decode data with version {} and overwinter {} into a known transaction version", version, overwinter),
                ))
            }
        };

        Ok(tx)
    }
}

/// A V1 transaction.
#[derive(Debug, PartialEq, Clone)]
pub struct TxV1 {
    tx_in: Vec<TxIn>,
    tx_out: Vec<TxOut>,

    // TODO: newtype?
    lock_time: u32,
}

impl Codec for TxV1 {
    fn encode<B: BufMut>(&self, buffer: &mut B) -> io::Result<()> {
        self.tx_in.encode(buffer)?;
        self.tx_out.encode(buffer)?;

        buffer.put_u32_le(self.lock_time);

        Ok(())
    }

    fn decode<B: Buf>(bytes: &mut B) -> io::Result<Self> {
        let tx_in = Vec::<TxIn>::decode(bytes)?;
        let tx_out = Vec::<TxOut>::decode(bytes)?;

        let lock_time = u32::from_le_bytes(read_n_bytes(bytes)?);

        Ok(Self {
            tx_in,
            tx_out,
            lock_time,
        })
    }
}

/// A V2 transaction.
#[derive(Debug, PartialEq, Clone)]
pub struct TxV2 {
    tx_in: Vec<TxIn>,
    tx_out: Vec<TxOut>,

    lock_time: u32,

    // BCTV14
    join_split: Vec<JoinSplit>,

    // Only present if the join_split count > 0.
    join_split_pub_key: Option<[u8; 32]>,
    join_split_sig: Option<[u8; 32]>,
}

impl Codec for TxV2 {
    fn encode<B: BufMut>(&self, buffer: &mut B) -> io::Result<()> {
        self.tx_in.encode(buffer)?;
        self.tx_out.encode(buffer)?;

        buffer.put_u32_le(self.lock_time);

        VarInt(self.join_split.len()).encode(buffer)?;
        for description in &self.join_split {
            // Encode join split description.
            description.encode(buffer)?;
        }

        if !self.join_split.is_empty() {
            // Must be present.
            buffer.put_slice(&self.join_split_pub_key.unwrap());
            buffer.put_slice(&self.join_split_sig.unwrap());
        }

        Ok(())
    }

    fn decode<B: Buf>(bytes: &mut B) -> io::Result<Self> {
        let tx_in = Vec::<TxIn>::decode(bytes)?;
        let tx_out = Vec::<TxOut>::decode(bytes)?;
        let lock_time = u32::from_le_bytes(read_n_bytes(bytes)?);

        let join_split_count = *VarInt::decode(bytes)?;
        let mut join_split = Vec::with_capacity(join_split_count);

        for _ in 0..join_split_count {
            let description = JoinSplit::decode_bctv14(bytes)?;
            join_split.push(description);
        }

        let (join_split_pub_key, join_split_sig) = if join_split_count > 0 {
            if bytes.remaining() < 64 {
                return Err(io::ErrorKind::InvalidData.into());
            }

            let mut pub_key = [0u8; 32];
            bytes.copy_to_slice(&mut pub_key);

            let mut sig = [0u8; 32];
            bytes.copy_to_slice(&mut sig);

            (Some(pub_key), Some(sig))
        } else {
            (None, None)
        };

        Ok(Self {
            tx_in,
            tx_out,
            lock_time,
            join_split,
            join_split_pub_key,
            join_split_sig,
        })
    }
}

/// A V3 transaction.
#[derive(Debug, PartialEq, Clone)]
pub struct TxV3 {
    group_id: u32,

    tx_in: Vec<TxIn>,
    tx_out: Vec<TxOut>,

    lock_time: u32,
    expiry_height: u32,

    // BCTV14
    join_split: Vec<JoinSplit>,

    // Only present if the join_split count > 0.
    join_split_pub_key: Option<[u8; 32]>,
    join_split_sig: Option<[u8; 32]>,
}

impl Codec for TxV3 {
    fn encode<B: BufMut>(&self, buffer: &mut B) -> io::Result<()> {
        buffer.put_u32_le(self.group_id);

        self.tx_in.encode(buffer)?;
        self.tx_out.encode(buffer)?;

        buffer.put_u32_le(self.lock_time);
        buffer.put_u32_le(self.expiry_height);

        VarInt(self.join_split.len()).encode(buffer)?;
        for description in &self.join_split {
            // Encode join split description.
            description.encode(buffer)?;
        }

        if !self.join_split.is_empty() {
            // Must be present.
            buffer.put_slice(&self.join_split_pub_key.unwrap());
            buffer.put_slice(&self.join_split_sig.unwrap());
        }

        Ok(())
    }

    fn decode<B: Buf>(bytes: &mut B) -> io::Result<Self> {
        let group_id = u32::from_le_bytes(read_n_bytes(bytes)?);

        let tx_in = Vec::<TxIn>::decode(bytes)?;
        let tx_out = Vec::<TxOut>::decode(bytes)?;
        let lock_time = u32::from_le_bytes(read_n_bytes(bytes)?);
        let expiry_height = u32::from_le_bytes(read_n_bytes(bytes)?);

        let join_split_count = *VarInt::decode(bytes)?;
        let mut join_split = Vec::with_capacity(join_split_count);

        for _ in 0..join_split_count {
            let description = JoinSplit::decode_bctv14(bytes)?;
            join_split.push(description);
        }

        let (join_split_pub_key, join_split_sig) = if join_split_count > 0 {
            if bytes.remaining() < 64 {
                return Err(io::ErrorKind::InvalidData.into());
            }

            let mut pub_key = [0u8; 32];
            bytes.copy_to_slice(&mut pub_key);

            let mut sig = [0u8; 32];
            bytes.copy_to_slice(&mut sig);

            (Some(pub_key), Some(sig))
        } else {
            (None, None)
        };

        Ok(Self {
            group_id,
            tx_in,
            tx_out,
            lock_time,
            expiry_height,
            join_split,
            join_split_pub_key,
            join_split_sig,
        })
    }
}

/// A V4 transaction.
#[derive(Debug, PartialEq, Clone)]
pub struct TxV4 {
    group_id: u32,

    tx_in: Vec<TxIn>,
    tx_out: Vec<TxOut>,

    lock_time: u32,
    expiry_height: u32,

    value_balance_sapling: i64,
    spends_sapling: Vec<SpendDescription>,
    outputs_sapling: Vec<SaplingOutput>,

    // Groth16
    join_split: Vec<JoinSplit>,

    // Only present if the join_split count > 0.
    join_split_pub_key: Option<[u8; 32]>,
    join_split_sig: Option<[u8; 32]>,

    // Present if and only if spends_sapling_count + outputs_sapling_count > 0.
    binding_sig_sapling: Option<[u8; 64]>,
}

impl Codec for TxV4 {
    fn encode<B: BufMut>(&self, buffer: &mut B) -> io::Result<()> {
        buffer.put_u32_le(self.group_id);

        self.tx_in.encode(buffer)?;
        self.tx_out.encode(buffer)?;

        buffer.put_u32_le(self.lock_time);
        buffer.put_u32_le(self.expiry_height);

        buffer.put_i64_le(self.value_balance_sapling);
        self.spends_sapling.encode(buffer)?;
        self.outputs_sapling.encode(buffer)?;

        VarInt(self.join_split.len()).encode(buffer)?;
        for description in &self.join_split {
            // Encode join split description.
            description.encode(buffer)?;
        }

        if !self.join_split.is_empty() {
            // Must be present.
            buffer.put_slice(&self.join_split_pub_key.unwrap());
            buffer.put_slice(&self.join_split_sig.unwrap());
        }

        if !self.spends_sapling.is_empty() || !self.outputs_sapling.is_empty() {
            // Must be present.
            buffer.put_slice(&self.binding_sig_sapling.unwrap());
        }

        Ok(())
    }

    fn decode<B: Buf>(bytes: &mut B) -> io::Result<Self> {
        let group_id = u32::from_le_bytes(read_n_bytes(bytes)?);

        let tx_in = Vec::<TxIn>::decode(bytes)?;
        let tx_out = Vec::<TxOut>::decode(bytes)?;
        let lock_time = u32::from_le_bytes(read_n_bytes(bytes)?);
        let expiry_height = u32::from_le_bytes(read_n_bytes(bytes)?);

        let value_balance_sapling = i64::from_le_bytes(read_n_bytes(bytes)?);
        let spends_sapling = Vec::<SpendDescription>::decode(bytes)?;
        let outputs_sapling = Vec::<SaplingOutput>::decode(bytes)?;

        let join_split_count = VarInt::decode(bytes)?;
        let mut join_split = Vec::with_capacity(*join_split_count);

        for _ in 0..*join_split_count {
            let description = JoinSplit::decode_groth16(bytes)?;
            join_split.push(description);
        }

        let (join_split_pub_key, join_split_sig) = if *join_split_count > 0 {
            if bytes.remaining() < 64 {
                return Err(io::ErrorKind::InvalidData.into());
            }

            let mut pub_key = [0u8; 32];
            bytes.copy_to_slice(&mut pub_key);

            let mut sig = [0u8; 32];
            bytes.copy_to_slice(&mut sig);

            (Some(pub_key), Some(sig))
        } else {
            (None, None)
        };

        let binding_sig_sapling = if !spends_sapling.is_empty() || !outputs_sapling.is_empty() {
            Some(read_n_bytes(bytes)?)
        } else {
            None
        };

        Ok(Self {
            group_id,
            tx_in,
            tx_out,
            lock_time,
            expiry_height,
            value_balance_sapling,
            spends_sapling,
            outputs_sapling,
            join_split,
            join_split_pub_key,
            join_split_sig,
            binding_sig_sapling,
        })
    }
}

#[derive(Debug, PartialEq, Clone)]
struct TxIn {
    // Outpoint object (previous output transaction reference).
    prev_out_hash: Hash,
    prev_out_index: u32,

    script_len: VarInt,
    script: Vec<u8>,

    // Is currently unused in bitcoin, not sure about Zcash.
    sequence: u32,
}

impl Codec for TxIn {
    fn encode<B: BufMut>(&self, buffer: &mut B) -> io::Result<()> {
        self.prev_out_hash.encode(buffer)?;
        buffer.put_u32_le(self.prev_out_index);

        self.script_len.encode(buffer)?;
        buffer.put_slice(&self.script);

        buffer.put_u32_le(self.sequence);

        Ok(())
    }

    fn decode<B: Buf>(bytes: &mut B) -> io::Result<Self> {
        let prev_out_hash = Hash::decode(bytes)?;
        let prev_out_index = u32::from_le_bytes(read_n_bytes(bytes)?);

        let script_len = VarInt::decode(bytes)?;

        if bytes.remaining() < script_len.0 {
            return Err(io::ErrorKind::InvalidData.into());
        }

        let mut script = vec![0u8; script_len.0];
        bytes.copy_to_slice(&mut script);

        let sequence = u32::from_le_bytes(read_n_bytes(bytes)?);

        Ok(Self {
            prev_out_hash,
            prev_out_index,
            script_len,
            script,
            sequence,
        })
    }
}

#[derive(Debug, PartialEq, Clone)]
struct TxOut {
    value: i64,
    pk_script_len: VarInt,
    pk_script: Vec<u8>,
}

impl Codec for TxOut {
    fn encode<B: BufMut>(&self, buffer: &mut B) -> io::Result<()> {
        buffer.put_i64_le(self.value);
        self.pk_script_len.encode(buffer)?;
        buffer.put_slice(&self.pk_script);

        Ok(())
    }

    fn decode<B: Buf>(bytes: &mut B) -> io::Result<Self> {
        let value = i64::from_le_bytes(read_n_bytes(bytes)?);
        let pk_script_len = VarInt::decode(bytes)?;

        if bytes.remaining() < pk_script_len.0 {
            return Err(io::ErrorKind::InvalidData.into());
        }

        let mut pk_script = vec![0u8; pk_script_len.0];
        bytes.copy_to_slice(&mut pk_script);

        Ok(Self {
            value,
            pk_script_len,
            pk_script,
        })
    }
}

#[derive(Debug, PartialEq, Clone)]
struct JoinSplit {
    pub_old: u64,
    pub_new: u64,
    anchor: [u8; 32],
    // Two nullifiers are present, each 32 bytes long.
    nullifiers: [u8; 64],
    // Two commitments are present, each 32 bytes long.
    commitments: [u8; 64],
    ephemeral_key: [u8; 32],
    random_seed: [u8; 32],
    // Two tags are present, each 32 bytes long.
    vmacs: [u8; 64],
    // BCTV14 or Groth16, depending on the transaction version.
    zkproof: Zkproof,
    // Two cyphertex components are present, each 601 bytes long.
    enc_cyphertexts: [u8; 1202],
}

impl JoinSplit {
    fn encode<B: BufMut>(&self, buffer: &mut B) -> io::Result<()> {
        buffer.put_u64_le(self.pub_old);
        buffer.put_u64_le(self.pub_new);

        buffer.put_slice(&self.anchor);
        buffer.put_slice(&self.nullifiers);
        buffer.put_slice(&self.commitments);
        buffer.put_slice(&self.ephemeral_key);
        buffer.put_slice(&self.random_seed);
        buffer.put_slice(&self.vmacs);

        self.zkproof.encode(buffer)?;
        buffer.put_slice(&self.enc_cyphertexts);

        Ok(())
    }

    fn decode_bctv14<B: Buf>(bytes: &mut B) -> io::Result<Self> {
        // TODO: deduplicate (might require generics).
        let pub_old = u64::from_le_bytes(read_n_bytes(bytes)?);
        let pub_new = u64::from_le_bytes(read_n_bytes(bytes)?);

        let anchor = read_n_bytes(bytes)?;
        let nullifiers = read_n_bytes(bytes)?;
        let commitments = read_n_bytes(bytes)?;
        let ephemeral_key = read_n_bytes(bytes)?;
        let random_seed = read_n_bytes(bytes)?;
        let vmacs = read_n_bytes(bytes)?;

        let zkproof = Zkproof::BCTV14(read_n_bytes(bytes)?);
        let enc_cyphertexts = read_n_bytes(bytes)?;

        Ok(Self {
            pub_old,
            pub_new,
            anchor,
            nullifiers,
            commitments,
            ephemeral_key,
            random_seed,
            vmacs,
            zkproof,
            enc_cyphertexts,
        })
    }

    fn decode_groth16<B: Buf>(bytes: &mut B) -> io::Result<Self> {
        let pub_old = u64::from_le_bytes(read_n_bytes(bytes)?);
        let pub_new = u64::from_le_bytes(read_n_bytes(bytes)?);

        let anchor = read_n_bytes(bytes)?;
        let nullifiers = read_n_bytes(bytes)?;
        let commitments = read_n_bytes(bytes)?;
        let ephemeral_key = read_n_bytes(bytes)?;
        let random_seed = read_n_bytes(bytes)?;
        let vmacs = read_n_bytes(bytes)?;

        let zkproof = Zkproof::Groth16(read_n_bytes(bytes)?);
        let enc_cyphertexts = read_n_bytes(bytes)?;

        Ok(Self {
            pub_old,
            pub_new,
            anchor,
            nullifiers,
            commitments,
            ephemeral_key,
            random_seed,
            vmacs,
            zkproof,
            enc_cyphertexts,
        })
    }
}

// TODO: rethink abstraction.
#[derive(Debug, PartialEq, Clone)]
enum Zkproof {
    BCTV14([u8; 296]),
    Groth16([u8; 192]),
}

impl Zkproof {
    fn encode<B: BufMut>(&self, buffer: &mut B) -> io::Result<()> {
        match self {
            Self::BCTV14(bytes) => buffer.put_slice(&*bytes),
            Self::Groth16(bytes) => buffer.put_slice(&*bytes),
        }

        Ok(())
    }
}

#[derive(Debug, PartialEq, Clone)]
struct SpendDescription {
    cv: [u8; 32],
    anchor: [u8; 32],
    nullifier: [u8; 32],
    rk: [u8; 32],
    // Groth16 only.
    zkproof: [u8; 192],
    spend_auth_sig: [u8; 64],
}

impl Codec for SpendDescription {
    fn encode<B: BufMut>(&self, buffer: &mut B) -> io::Result<()> {
        buffer.put_slice(&self.cv);
        buffer.put_slice(&self.anchor);
        buffer.put_slice(&self.nullifier);
        buffer.put_slice(&self.rk);
        buffer.put_slice(&self.zkproof);
        buffer.put_slice(&self.spend_auth_sig);

        Ok(())
    }

    fn decode<B: Buf>(bytes: &mut B) -> io::Result<Self> {
        let cv = read_n_bytes(bytes)?;
        let anchor = read_n_bytes(bytes)?;
        let nullifier = read_n_bytes(bytes)?;
        let rk = read_n_bytes(bytes)?;
        let zkproof = read_n_bytes(bytes)?;
        let spend_auth_sig = read_n_bytes(bytes)?;

        Ok(Self {
            cv,
            anchor,
            nullifier,
            rk,
            zkproof,
            spend_auth_sig,
        })
    }
}

#[derive(Debug, PartialEq, Clone)]
struct SaplingOutput {
    cv: [u8; 32],
    cmu: [u8; 32],
    ephemeral_key: [u8; 32],
    enc_cyphertext: [u8; 580],
    out_cyphertext: [u8; 80],
    zkproof: [u8; 192],
}

impl Codec for SaplingOutput {
    fn encode<B: BufMut>(&self, buffer: &mut B) -> io::Result<()> {
        buffer.put_slice(&self.cv);
        buffer.put_slice(&self.cmu);
        buffer.put_slice(&self.ephemeral_key);
        buffer.put_slice(&self.enc_cyphertext);
        buffer.put_slice(&self.out_cyphertext);
        buffer.put_slice(&self.zkproof);

        Ok(())
    }

    fn decode<B: Buf>(bytes: &mut B) -> io::Result<Self> {
        let cv = read_n_bytes(bytes)?;
        let cmu = read_n_bytes(bytes)?;
        let ephemeral_key = read_n_bytes(bytes)?;
        let enc_cyphertext = read_n_bytes(bytes)?;
        let out_cyphertext = read_n_bytes(bytes)?;
        let zkproof = read_n_bytes(bytes)?;

        Ok(Self {
            cv,
            cmu,
            ephemeral_key,
            enc_cyphertext,
            out_cyphertext,
            zkproof,
        })
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    use io::Cursor;

    #[test]
    #[ignore]
    fn empty_transaction_v1_round_trip() {
        let tx_v1 = Tx::V1(TxV1 {
            tx_in: Vec::new(),
            tx_out: Vec::new(),
            lock_time: 500_000_000,
        });

        let mut bytes = Vec::new();
        tx_v1.encode(&mut bytes).unwrap();

        assert_eq!(tx_v1, Tx::decode(&mut Cursor::new(&bytes)).unwrap());
    }

    #[test]
    #[ignore]
    fn empty_transaction_v2_round_trip() {
        let tx_v2 = Tx::V2(TxV2 {
            tx_in: Vec::new(),
            tx_out: Vec::new(),
            lock_time: 500_000_000,
            join_split: Vec::new(),
            join_split_pub_key: None,
            join_split_sig: None,
        });

        let mut bytes = Vec::new();
        tx_v2.encode(&mut bytes).unwrap();

        assert_eq!(tx_v2, Tx::decode(&mut Cursor::new(&bytes)).unwrap());
    }

    #[test]
    #[ignore]
    fn empty_transaction_v3_round_trip() {
        let tx_v3 = Tx::V3(TxV3 {
            group_id: 0,
            tx_in: Vec::new(),
            tx_out: Vec::new(),
            lock_time: 500_000_000,
            expiry_height: 500_000_000,
            join_split: Vec::new(),
            join_split_pub_key: None,
            join_split_sig: None,
        });

        let mut bytes = Vec::new();
        tx_v3.encode(&mut bytes).unwrap();

        assert_eq!(tx_v3, Tx::decode(&mut Cursor::new(&bytes)).unwrap());
    }

    #[test]
    #[ignore]
    fn empty_transaction_v4_round_trip() {
        let tx_v4 = Tx::V4(TxV4 {
            group_id: 0,
            tx_in: Vec::new(),
            tx_out: Vec::new(),
            lock_time: 500_000_000,
            expiry_height: 500_000_000,
            value_balance_sapling: 0,
            spends_sapling: Vec::new(),
            outputs_sapling: Vec::new(),
            join_split: Vec::new(),
            join_split_pub_key: None,
            join_split_sig: None,
            binding_sig_sapling: None,
        });

        let mut bytes = Vec::new();
        tx_v4.encode(&mut bytes).unwrap();

        assert_eq!(tx_v4, Tx::decode(&mut Cursor::new(&bytes)).unwrap());
    }
}
