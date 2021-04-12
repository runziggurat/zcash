use crate::protocol::payload::{read_n_bytes, Hash, VarInt};

use std::io::{self, Cursor, Read, Write};

#[derive(Debug, PartialEq)]
pub enum Tx {
    V1(TxV1),
    V2(TxV2),
    // V3(TxV3),
    // V4(TxV4),
    // Not yet stabalised.
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
            _ => unimplemented!(),
        }

        Ok(())
    }

    pub fn decode(bytes: &mut Cursor<&[u8]>) -> io::Result<Self> {
        let (version, overwinter) = {
            const LOW_31_BITS: u32 = (1 << 31) - 1;
            let header = u32::from_le_bytes(read_n_bytes(bytes)?);

            // Extract transaction version and check if overwinter flag is set.
            (header & LOW_31_BITS, header >> 31 != 0)
        };

        let tx = match (version, overwinter) {
            (1, false) => Self::V1(TxV1::decode(bytes)?),
            (2, false) => Self::V2(TxV2::decode(bytes)?),
            _ => unimplemented!(),
        };

        Ok(tx)
    }
}

#[derive(Debug, PartialEq)]
pub struct TxV1 {
    tx_in_count: VarInt,
    tx_in: Vec<TxIn>,

    tx_out_count: VarInt,
    tx_out: Vec<TxOut>,

    // Newtype?
    lock_time: u32,
}

impl TxV1 {
    fn encode(&self, buffer: &mut Vec<u8>) -> io::Result<()> {
        self.tx_in_count.encode(buffer)?;
        for input in &self.tx_in {
            input.encode(buffer)?;
        }

        self.tx_out_count.encode(buffer)?;
        for output in &self.tx_out {
            output.encode(buffer)?;
        }

        buffer.write_all(&self.lock_time.to_le_bytes())?;

        Ok(())
    }

    fn decode(bytes: &mut Cursor<&[u8]>) -> io::Result<Self> {
        let tx_in_count = VarInt::decode(bytes)?;
        let mut tx_in = Vec::with_capacity(tx_in_count.0);

        for _ in 0..tx_in_count.0 {
            let input = TxIn::decode(bytes)?;
            tx_in.push(input);
        }

        let tx_out_count = VarInt::decode(bytes)?;
        let mut tx_out = Vec::with_capacity(tx_out_count.0);

        for _ in 0..tx_out_count.0 {
            let output = TxOut::decode(bytes)?;
            tx_out.push(output);
        }

        let lock_time = u32::from_le_bytes(read_n_bytes(bytes)?);

        Ok(Self {
            tx_in_count,
            tx_in,
            tx_out_count,
            tx_out,
            lock_time,
        })
    }
}

#[derive(Debug, PartialEq)]
pub struct TxV2 {
    tx_in_count: VarInt,
    tx_in: Vec<TxIn>,

    tx_out_count: VarInt,
    tx_out: Vec<TxOut>,

    lock_time: u32,

    // BCTV14
    join_split_count: VarInt,
    join_split: Vec<JoinSplit>,

    // Only present if the join_split count > 0.
    join_split_pub_key: Option<[u8; 32]>,
    join_split_sig: Option<[u8; 32]>,
}

impl TxV2 {
    fn encode(&self, buffer: &mut Vec<u8>) -> io::Result<()> {
        self.tx_in_count.encode(buffer)?;
        for input in &self.tx_in {
            input.encode(buffer)?;
        }

        self.tx_out_count.encode(buffer)?;
        for output in &self.tx_out {
            output.encode(buffer)?;
        }

        buffer.write_all(&self.lock_time.to_le_bytes())?;

        self.join_split_count.encode(buffer)?;
        for description in &self.join_split {
            // Encode join split description.
            description.encode(buffer)?;
        }

        if self.join_split_count.0 > 0 {
            // Must be present.
            buffer.write_all(&self.join_split_pub_key.unwrap())?;
            buffer.write_all(&self.join_split_sig.unwrap())?;
        }

        Ok(())
    }

    fn decode(bytes: &mut Cursor<&[u8]>) -> io::Result<Self> {
        let tx_in_count = VarInt::decode(bytes)?;
        let mut tx_in = Vec::with_capacity(tx_in_count.0);

        for _ in 0..tx_in_count.0 {
            let input = TxIn::decode(bytes)?;
            tx_in.push(input);
        }

        let tx_out_count = VarInt::decode(bytes)?;
        let mut tx_out = Vec::with_capacity(tx_out_count.0);

        for _ in 0..tx_out_count.0 {
            let output = TxOut::decode(bytes)?;
            tx_out.push(output);
        }

        let lock_time = u32::from_le_bytes(read_n_bytes(bytes)?);

        let join_split_count = VarInt::decode(bytes)?;
        let mut join_split = Vec::with_capacity(join_split_count.0);

        for _ in 0..join_split_count.0 {
            let description = JoinSplit::decode(bytes)?;
            join_split.push(description);
        }

        let (join_split_pub_key, join_split_sig) = if join_split_count.0 > 0 {
            // Todo: consider making these concrete types.
            let mut pub_key = [0u8; 32];
            bytes.read_exact(&mut pub_key)?;

            let mut sig = [0u8; 32];
            bytes.read_exact(&mut sig)?;

            (Some(pub_key), Some(sig))
        } else {
            (None, None)
        };

        Ok(Self {
            tx_in_count,
            tx_in,
            tx_out_count,
            tx_out,
            lock_time,
            join_split_count,
            join_split,
            join_split_pub_key,
            join_split_sig,
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

    fn decode(bytes: &mut Cursor<&[u8]>) -> io::Result<Self> {
        let pub_old = u64::from_le_bytes(read_n_bytes(bytes)?);
        let pub_new = u64::from_le_bytes(read_n_bytes(bytes)?);

        let anchor = read_n_bytes(bytes)?;
        let nullifiers = read_n_bytes(bytes)?;
        let commitments = read_n_bytes(bytes)?;
        let ephemeral_key = read_n_bytes(bytes)?;
        let random_seed = read_n_bytes(bytes)?;
        let vmacs = read_n_bytes(bytes)?;

        let zkproof = Zkproof::decode(bytes)?;
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

    fn decode(bytes: &mut Cursor<&[u8]>) -> io::Result<Self> {
        // Maybe there's a better way? Didn't want to reach for generics straight away and since
        // the two last keys in JoinSplit are constant size...
        let still_to_read = bytes.get_ref().len() - bytes.position() as usize;

        let proof = match still_to_read {
            // 296 BCTV14 + 1202 cyphertext components.
            1498 => Self::BCTV14(read_n_bytes(bytes)?),
            // 192 Groth16 + 1202 cyphertext components.
            1394 => Self::Groth16(read_n_bytes(bytes)?),
            _ => panic!("Couldn't decode zk-proof into BCTV14 or Groth16."),
        };

        Ok(proof)
    }
}

//
// struct TxV3 {
//     header: u32,
//     group_id: u32,
//
//     tx_in_count: VarInt,
//     tx_in: Vec<TxIn>,
//
//     tx_out_count: VarInt,
//     tx_out: Vec<TxOut>,
//
//     lock_time: u32,
//     expiry_height: u32,
//
//     // BCTV14
//     join_split_count: VarInt,
//     join_split: Vec<JoinSplit>,
//
//     // Only present if the join_split count > 0.
//     join_split_pub_key: Option<[u8; 32]>,
//     join_split_sig: Option<[u8; 32]>,
// }
//
// struct TxV4 {
//     header: u32,
//     group_id: u32,
//
//     tx_in_count: VarInt,
//     tx_in: Vec<TxIn>,
//
//     tx_out_count: VarInt,
//     tx_out: Vec<TxOut>,
//
//     lock_time: u32,
//     expiry_height: u32,
//
//     value_balance_sapling: i64,
//     spends_sapling_count: VarInt,
//     spends_sapling: Vec<SpendDescription>,
//     outputs_sapling_count: VarInt,
//     outputs_sapling: Vec<SaplingOutput>,
//
//     // Groth16
//     join_split_count: VarInt,
//     join_split: Vec<JoinSplit>,
//
//     // Only present if the join_split count > 0.
//     join_split_pub_key: Option<[u8; 32]>,
//     join_split_sig: Option<[u8; 32]>,
//
//     // Present if and only if spends_sapling_count + outputs_sapling_count > 0.
//     binding_sig_sapling: Option<[u8; 64]>,
// }

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    #[ignore]
    fn empty_transaction_v1_round_trip() {
        let tx_v1 = Tx::V1(TxV1 {
            tx_in_count: VarInt(0),
            tx_in: Vec::new(),
            tx_out_count: VarInt(0),
            tx_out: Vec::new(),
            lock_time: 500_000_000,
        });

        let mut bytes = Vec::new();
        tx_v1.encode(&mut bytes).unwrap();

        assert_eq!(tx_v1, Tx::decode(&mut Cursor::new(&bytes)).unwrap());
    }
}
