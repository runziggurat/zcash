use crate::protocol::payload::{read_n_bytes, Hash, VarInt};

use std::io::{self, Cursor, Read, Write};

/// A Zcash transaction ([spec](https://zips.z.cash/protocol/canopy.pdf#txnencodingandconsensus)).
///
/// Supports V1-V4, V5 isn't yet stable.
#[derive(Debug, PartialEq)]
pub enum Tx {
    V1(TxV1),
    V2(TxV2),
    V3(TxV3),
    V4(TxV4),
    // Not yet stabilised.
    V5,
}

impl Tx {
    pub fn encode(&self, buffer: &mut Vec<u8>) -> io::Result<()> {
        match self {
            Tx::V1(tx) => {
                // The overwintered flag is NOT set.
                buffer.write_all(&1u32.to_le_bytes())?;
                tx.encode(buffer)?;
            }
            Tx::V2(tx) => {
                // The overwintered flag is NOT set.
                buffer.write_all(&2u32.to_le_bytes())?;
                tx.encode(buffer)?;
            }
            Tx::V3(tx) => {
                // The overwintered flag IS set.
                buffer.write_all(&(3u32 | 1 << 31).to_le_bytes())?;
                tx.encode(buffer)?;
            }
            Tx::V4(tx) => {
                // The overwintered flag IS set.
                buffer.write_all(&(4u32 | 1 << 31).to_le_bytes())?;
                tx.encode(buffer)?;
            }
            Tx::V5 => unimplemented!(),
        }

        Ok(())
    }

    pub fn decode(bytes: &mut Cursor<&[u8]>) -> io::Result<Self> {
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

#[derive(Debug, PartialEq)]
pub struct TxV1 {
    tx_in: Vec<TxIn>,
    tx_out: Vec<TxOut>,

    // TODO: newtype?
    lock_time: u32,
}

impl TxV1 {
    fn encode(&self, buffer: &mut Vec<u8>) -> io::Result<()> {
        VarInt(self.tx_in.len()).encode(buffer)?;
        for input in &self.tx_in {
            input.encode(buffer)?;
        }

        VarInt(self.tx_out.len()).encode(buffer)?;
        for output in &self.tx_out {
            output.encode(buffer)?;
        }

        buffer.write_all(&self.lock_time.to_le_bytes())?;

        Ok(())
    }

    fn decode(bytes: &mut Cursor<&[u8]>) -> io::Result<Self> {
        let tx_in_count = *VarInt::decode(bytes)?;
        let mut tx_in = Vec::with_capacity(tx_in_count);

        for _ in 0..tx_in_count {
            let input = TxIn::decode(bytes)?;
            tx_in.push(input);
        }

        let tx_out_count = *VarInt::decode(bytes)?;
        let mut tx_out = Vec::with_capacity(tx_out_count);

        for _ in 0..tx_out_count {
            let output = TxOut::decode(bytes)?;
            tx_out.push(output);
        }

        let lock_time = u32::from_le_bytes(read_n_bytes(bytes)?);

        Ok(Self {
            tx_in,
            tx_out,
            lock_time,
        })
    }
}

#[derive(Debug, PartialEq)]
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

impl TxV2 {
    fn encode(&self, buffer: &mut Vec<u8>) -> io::Result<()> {
        VarInt(self.tx_in.len()).encode(buffer)?;
        for input in &self.tx_in {
            input.encode(buffer)?;
        }

        VarInt(self.tx_out.len()).encode(buffer)?;
        for output in &self.tx_out {
            output.encode(buffer)?;
        }

        buffer.write_all(&self.lock_time.to_le_bytes())?;

        VarInt(self.join_split.len()).encode(buffer)?;
        for description in &self.join_split {
            // Encode join split description.
            description.encode(buffer)?;
        }

        if !self.join_split.is_empty() {
            // Must be present.
            buffer.write_all(&self.join_split_pub_key.unwrap())?;
            buffer.write_all(&self.join_split_sig.unwrap())?;
        }

        Ok(())
    }

    fn decode(bytes: &mut Cursor<&[u8]>) -> io::Result<Self> {
        let tx_in_count = *VarInt::decode(bytes)?;
        let mut tx_in = Vec::with_capacity(tx_in_count);

        for _ in 0..tx_in_count {
            let input = TxIn::decode(bytes)?;
            tx_in.push(input);
        }

        let tx_out_count = *VarInt::decode(bytes)?;
        let mut tx_out = Vec::with_capacity(tx_out_count);

        for _ in 0..tx_out_count {
            let output = TxOut::decode(bytes)?;
            tx_out.push(output);
        }

        let lock_time = u32::from_le_bytes(read_n_bytes(bytes)?);

        let join_split_count = *VarInt::decode(bytes)?;
        let mut join_split = Vec::with_capacity(join_split_count);

        for _ in 0..join_split_count {
            let description = JoinSplit::decode_bctv14(bytes)?;
            join_split.push(description);
        }

        let (join_split_pub_key, join_split_sig) = if join_split_count > 0 {
            let mut pub_key = [0u8; 32];
            bytes.read_exact(&mut pub_key)?;

            let mut sig = [0u8; 32];
            bytes.read_exact(&mut sig)?;

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

#[derive(Debug, PartialEq)]
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

impl TxV3 {
    fn encode(&self, buffer: &mut Vec<u8>) -> io::Result<()> {
        buffer.write_all(&self.group_id.to_le_bytes())?;

        VarInt(self.tx_in.len()).encode(buffer)?;
        for input in &self.tx_in {
            input.encode(buffer)?;
        }

        VarInt(self.tx_out.len()).encode(buffer)?;
        for output in &self.tx_out {
            output.encode(buffer)?;
        }

        buffer.write_all(&self.lock_time.to_le_bytes())?;
        buffer.write_all(&self.expiry_height.to_le_bytes())?;

        VarInt(self.join_split.len()).encode(buffer)?;
        for description in &self.join_split {
            // Encode join split description.
            description.encode(buffer)?;
        }

        if !self.join_split.is_empty() {
            // Must be present.
            buffer.write_all(&self.join_split_pub_key.unwrap())?;
            buffer.write_all(&self.join_split_sig.unwrap())?;
        }

        Ok(())
    }

    fn decode(bytes: &mut Cursor<&[u8]>) -> io::Result<Self> {
        let group_id = u32::from_le_bytes(read_n_bytes(bytes)?);

        let tx_in_count = *VarInt::decode(bytes)?;
        let mut tx_in = Vec::with_capacity(tx_in_count);

        for _ in 0..tx_in_count {
            let input = TxIn::decode(bytes)?;
            tx_in.push(input);
        }

        let tx_out_count = *VarInt::decode(bytes)?;
        let mut tx_out = Vec::with_capacity(tx_out_count);

        for _ in 0..tx_out_count {
            let output = TxOut::decode(bytes)?;
            tx_out.push(output);
        }

        let lock_time = u32::from_le_bytes(read_n_bytes(bytes)?);
        let expiry_height = u32::from_le_bytes(read_n_bytes(bytes)?);

        let join_split_count = *VarInt::decode(bytes)?;
        let mut join_split = Vec::with_capacity(join_split_count);

        for _ in 0..join_split_count {
            let description = JoinSplit::decode_bctv14(bytes)?;
            join_split.push(description);
        }

        let (join_split_pub_key, join_split_sig) = if join_split_count > 0 {
            let mut pub_key = [0u8; 32];
            bytes.read_exact(&mut pub_key)?;

            let mut sig = [0u8; 32];
            bytes.read_exact(&mut sig)?;

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

#[derive(Debug, PartialEq)]
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

impl TxV4 {
    fn encode(&self, buffer: &mut Vec<u8>) -> io::Result<()> {
        buffer.write_all(&self.group_id.to_le_bytes())?;

        VarInt(self.tx_in.len()).encode(buffer)?;
        for input in &self.tx_in {
            input.encode(buffer)?;
        }

        VarInt(self.tx_out.len()).encode(buffer)?;
        for output in &self.tx_out {
            output.encode(buffer)?;
        }

        buffer.write_all(&self.lock_time.to_le_bytes())?;
        buffer.write_all(&self.expiry_height.to_le_bytes())?;

        buffer.write_all(&self.value_balance_sapling.to_le_bytes())?;
        VarInt(self.spends_sapling.len()).encode(buffer)?;
        for spend in &self.spends_sapling {
            spend.encode(buffer)?;
        }

        VarInt(self.outputs_sapling.len()).encode(buffer)?;
        for output in &self.outputs_sapling {
            output.encode(buffer)?;
        }

        VarInt(self.join_split.len()).encode(buffer)?;
        for description in &self.join_split {
            // Encode join split description.
            description.encode(buffer)?;
        }

        if !self.join_split.is_empty() {
            // Must be present.
            buffer.write_all(&self.join_split_pub_key.unwrap())?;
            buffer.write_all(&self.join_split_sig.unwrap())?;
        }

        if !self.spends_sapling.is_empty() || !self.outputs_sapling.is_empty() {
            // Must be present.
            buffer.write_all(&self.binding_sig_sapling.unwrap())?
        }

        Ok(())
    }

    fn decode(bytes: &mut Cursor<&[u8]>) -> io::Result<Self> {
        let group_id = u32::from_le_bytes(read_n_bytes(bytes)?);

        let tx_in_count = VarInt::decode(bytes)?;
        let mut tx_in = Vec::with_capacity(*tx_in_count);

        for _ in 0..*tx_in_count {
            let input = TxIn::decode(bytes)?;
            tx_in.push(input);
        }

        let tx_out_count = VarInt::decode(bytes)?;
        let mut tx_out = Vec::with_capacity(*tx_out_count);

        for _ in 0..*tx_out_count {
            let output = TxOut::decode(bytes)?;
            tx_out.push(output);
        }

        let lock_time = u32::from_le_bytes(read_n_bytes(bytes)?);
        let expiry_height = u32::from_le_bytes(read_n_bytes(bytes)?);

        let value_balance_sapling = i64::from_le_bytes(read_n_bytes(bytes)?);
        let spends_sapling_count = VarInt::decode(bytes)?;
        let mut spends_sapling = Vec::with_capacity(*spends_sapling_count);
        for _ in 0..*spends_sapling_count {
            let spend = SpendDescription::decode(bytes)?;
            spends_sapling.push(spend);
        }

        let outputs_sapling_count = VarInt::decode(bytes)?;
        let mut outputs_sapling = Vec::with_capacity(*outputs_sapling_count);
        for _ in 0..*outputs_sapling_count {
            let output = SaplingOutput::decode(bytes)?;
            outputs_sapling.push(output);
        }

        let join_split_count = VarInt::decode(bytes)?;
        let mut join_split = Vec::with_capacity(*join_split_count);

        for _ in 0..*join_split_count {
            let description = JoinSplit::decode_groth16(bytes)?;
            join_split.push(description);
        }

        let (join_split_pub_key, join_split_sig) = if *join_split_count > 0 {
            let mut pub_key = [0u8; 32];
            bytes.read_exact(&mut pub_key)?;

            let mut sig = [0u8; 32];
            bytes.read_exact(&mut sig)?;

            (Some(pub_key), Some(sig))
        } else {
            (None, None)
        };

        let binding_sig_sapling = if *spends_sapling_count + *outputs_sapling_count > 0 {
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

#[derive(Debug, PartialEq)]
struct TxIn {
    // Outpoint object (previous output transaction reference).
    prev_out_hash: Hash,
    prev_out_index: u32,

    script_len: VarInt,
    script: Vec<u8>,

    // Is currently unused in bitcoin, not sure about Zcash.
    sequence: u32,
}

impl TxIn {
    fn encode(&self, buffer: &mut Vec<u8>) -> io::Result<()> {
        self.prev_out_hash.encode(buffer)?;
        buffer.write_all(&self.prev_out_index.to_le_bytes())?;

        self.script_len.encode(buffer)?;
        buffer.write_all(&self.script)?;

        buffer.write_all(&self.sequence.to_le_bytes())?;

        Ok(())
    }

    fn decode(bytes: &mut Cursor<&[u8]>) -> io::Result<Self> {
        let prev_out_hash = Hash::decode(bytes)?;
        let prev_out_index = u32::from_le_bytes(read_n_bytes(bytes)?);

        let script_len = VarInt::decode(bytes)?;
        let mut script = vec![0u8; script_len.0];
        bytes.read_exact(&mut script)?;

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

#[derive(Debug, PartialEq)]
struct TxOut {
    value: i64,
    pk_script_len: VarInt,
    pk_script: Vec<u8>,
}

impl TxOut {
    fn encode(&self, buffer: &mut Vec<u8>) -> io::Result<()> {
        buffer.write_all(&self.value.to_le_bytes())?;
        self.pk_script_len.encode(buffer)?;
        buffer.write_all(&self.pk_script)?;

        Ok(())
    }

    fn decode(bytes: &mut Cursor<&[u8]>) -> io::Result<Self> {
        let value = i64::from_le_bytes(read_n_bytes(bytes)?);
        let pk_script_len = VarInt::decode(bytes)?;
        let mut pk_script = vec![0u8; pk_script_len.0];
        bytes.read_exact(&mut pk_script)?;

        Ok(Self {
            value,
            pk_script_len,
            pk_script,
        })
    }
}

#[derive(Debug, PartialEq)]
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
    fn encode(&self, buffer: &mut Vec<u8>) -> io::Result<()> {
        buffer.write_all(&self.pub_old.to_le_bytes())?;
        buffer.write_all(&self.pub_new.to_le_bytes())?;

        buffer.write_all(&self.anchor)?;
        buffer.write_all(&self.nullifiers)?;
        buffer.write_all(&self.commitments)?;
        buffer.write_all(&self.ephemeral_key)?;
        buffer.write_all(&self.random_seed)?;
        buffer.write_all(&self.vmacs)?;

        self.zkproof.encode(buffer)?;
        buffer.write_all(&self.enc_cyphertexts)?;

        Ok(())
    }

    fn decode_bctv14(bytes: &mut Cursor<&[u8]>) -> io::Result<Self> {
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

    fn decode_groth16(bytes: &mut Cursor<&[u8]>) -> io::Result<Self> {
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
#[derive(Debug, PartialEq)]
enum Zkproof {
    BCTV14([u8; 296]),
    Groth16([u8; 192]),
}

impl Zkproof {
    fn encode(&self, buffer: &mut Vec<u8>) -> io::Result<()> {
        match self {
            Self::BCTV14(bytes) => buffer.write_all(bytes)?,
            Self::Groth16(bytes) => buffer.write_all(bytes)?,
        }

        Ok(())
    }
}

#[derive(Debug, PartialEq)]
struct SpendDescription {
    cv: [u8; 32],
    anchor: [u8; 32],
    nullifier: [u8; 32],
    rk: [u8; 32],
    // Groth16 only.
    zkproof: [u8; 192],
    spend_auth_sig: [u8; 64],
}

impl SpendDescription {
    fn encode(&self, buffer: &mut Vec<u8>) -> io::Result<()> {
        buffer.write_all(&self.cv)?;
        buffer.write_all(&self.anchor)?;
        buffer.write_all(&self.nullifier)?;
        buffer.write_all(&self.rk)?;
        buffer.write_all(&self.zkproof)?;
        buffer.write_all(&self.spend_auth_sig)?;

        Ok(())
    }

    fn decode(bytes: &mut Cursor<&[u8]>) -> io::Result<Self> {
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

#[derive(Debug, PartialEq)]
struct SaplingOutput {
    cv: [u8; 32],
    cmu: [u8; 32],
    ephemeral_key: [u8; 32],
    enc_cyphertext: [u8; 580],
    out_cyphertext: [u8; 80],
    zkproof: [u8; 192],
}

impl SaplingOutput {
    fn encode(&self, buffer: &mut Vec<u8>) -> io::Result<()> {
        buffer.write_all(&self.cv)?;
        buffer.write_all(&self.cmu)?;
        buffer.write_all(&self.ephemeral_key)?;
        buffer.write_all(&self.enc_cyphertext)?;
        buffer.write_all(&self.out_cyphertext)?;
        buffer.write_all(&self.zkproof)?;

        Ok(())
    }

    fn decode(bytes: &mut Cursor<&[u8]>) -> io::Result<Self> {
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
