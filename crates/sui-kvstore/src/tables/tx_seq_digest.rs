// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

//! Direct mapping from `tx_sequence_number → (TransactionDigest, event_count, checkpoint_number)`.
//!
//! Row key layout: `[bit_reversed_tx_seq_be_u64]` — 8 bytes. Bit reversal
//! spreads monotonic transaction sequence numbers across the keyspace while
//! preserving a reversible point-lookup key.
//!
//! `event_count` lets readers enumerate a transaction's event_seqs without
//! reading the tx row itself — used by unfiltered event listing to discover
//! exactly the events contributing to a page.

use anyhow::bail;
use anyhow::{Context, Result};
use bytes::Bytes;
use integer_encoding::VarInt;
use sui_types::digests::TransactionDigest;
use sui_types::messages_checkpoint::CheckpointSequenceNumber;

pub const NAME: &str = "tx_seq_digest";

pub mod col {
    /// Raw 32-byte TransactionDigest.
    pub const DIGEST: &str = "d";
    /// Big-endian u32 count of events emitted by this transaction.
    pub const EVENT_COUNT: &str = "e";
    /// Varint-encoded checkpoint sequence number containing this transaction.
    pub const CHECKPOINT_NUMBER: &str = "c";
}

/// Row key: `tx_seq.reverse_bits().to_be_bytes()`.
pub fn encode_key(tx_seq: u64) -> Vec<u8> {
    tx_seq.reverse_bits().to_be_bytes().to_vec()
}

/// Decode a bit-reversed row key.
pub fn decode_key(key: &[u8]) -> Result<u64> {
    if key.len() != 8 {
        anyhow::bail!("tx_seq_digest key not 8 bytes (got {})", key.len());
    }
    Ok(u64::from_be_bytes(key.try_into().unwrap()).reverse_bits())
}

pub fn encode(
    digest: &TransactionDigest,
    event_count: u32,
    checkpoint_number: CheckpointSequenceNumber,
) -> [(&'static str, Bytes); 3] {
    [
        (col::DIGEST, Bytes::from(digest.inner().to_vec())),
        (
            col::EVENT_COUNT,
            Bytes::copy_from_slice(&event_count.to_be_bytes()),
        ),
        (
            col::CHECKPOINT_NUMBER,
            Bytes::from(checkpoint_number.encode_var_vec()),
        ),
    ]
}

pub fn decode(
    cells: &[(Bytes, Bytes)],
) -> Result<(TransactionDigest, u32, CheckpointSequenceNumber)> {
    let mut digest: Option<TransactionDigest> = None;
    let mut event_count: u32 = 0;
    let mut checkpoint_number = None;
    for (column, value) in cells {
        if column.as_ref() == col::DIGEST.as_bytes() {
            let bytes: [u8; 32] = value
                .as_ref()
                .try_into()
                .context("tx_seq_digest digest not 32 bytes")?;
            digest = Some(TransactionDigest::from(bytes));
        } else if column.as_ref() == col::EVENT_COUNT.as_bytes() {
            let bytes: [u8; 4] = value
                .as_ref()
                .try_into()
                .context("tx_seq_digest event_count not 4 bytes")?;
            event_count = u32::from_be_bytes(bytes);
        } else if column.as_ref() == col::CHECKPOINT_NUMBER.as_bytes() {
            checkpoint_number = Some(decode_checkpoint_number_value(value)?);
        }
    }
    Ok((
        digest.context("tx_seq_digest missing digest column")?,
        event_count,
        checkpoint_number.context("tx_seq_digest missing checkpoint_number column")?,
    ))
}

pub fn decode_checkpoint_number(cells: &[(Bytes, Bytes)]) -> Result<CheckpointSequenceNumber> {
    for (column, value) in cells {
        if column.as_ref() == col::CHECKPOINT_NUMBER.as_bytes() {
            return decode_checkpoint_number_value(value);
        }
    }

    bail!("tx_seq_digest missing checkpoint_number column")
}

fn decode_checkpoint_number_value(value: &Bytes) -> Result<CheckpointSequenceNumber> {
    let (checkpoint_number, bytes_read) = u64::decode_var(value.as_ref())
        .context("tx_seq_digest checkpoint_number is not a valid u64 varint")?;
    if bytes_read != value.len() {
        bail!("tx_seq_digest checkpoint_number has trailing bytes");
    }
    if checkpoint_number.required_space() != bytes_read {
        bail!("tx_seq_digest checkpoint_number is not canonical");
    }
    Ok(checkpoint_number)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn encode_key_layout() {
        for tx_seq in [0u64, 1, 63, 64, 65, 1_000_000, u64::MAX] {
            let k = encode_key(tx_seq);
            assert_eq!(k.len(), 8, "key must be 8 bytes for tx_seq={tx_seq}");
            assert_eq!(&k, &tx_seq.reverse_bits().to_be_bytes());
        }
    }

    #[test]
    fn encode_decode_round_trip() {
        for tx_seq in [0u64, 1, 63, 64, 65, 1_000_000, u64::MAX] {
            let k = encode_key(tx_seq);
            let got = decode_key(&k).expect("decode must accept freshly encoded key");
            assert_eq!(got, tx_seq);
        }
    }

    #[test]
    fn encode_decode_row_round_trip() {
        let digest = TransactionDigest::new([7; 32]);
        let event_count = 123_456;
        let checkpoint_number = 300_000_000;
        let cells = encode(&digest, event_count, checkpoint_number);

        assert_eq!(cells[1].1.as_ref(), &event_count.to_be_bytes());
        assert_eq!(cells[2].1.len(), 5);

        let cells = cells
            .into_iter()
            .map(|(column, value)| (Bytes::from_static(column.as_bytes()), value))
            .collect::<Vec<_>>();
        let (decoded_digest, decoded_event_count, decoded_checkpoint_number) =
            decode(&cells).unwrap();
        assert_eq!(decoded_digest, digest);
        assert_eq!(decoded_event_count, event_count);
        assert_eq!(decoded_checkpoint_number, checkpoint_number);
        assert_eq!(decode_checkpoint_number(&cells).unwrap(), checkpoint_number);
    }

    #[test]
    fn decode_key_rejects_wrong_length() {
        assert!(decode_key(&[0u8; 7]).is_err());
        assert!(decode_key(&[0u8; 9]).is_err());
        assert!(decode_key(&[]).is_err());
    }
}
