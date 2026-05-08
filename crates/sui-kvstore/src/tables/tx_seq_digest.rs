// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

//! Direct mapping from `tx_sequence_number → (TransactionDigest, event_count)`.
//!
//! Row key layout: `[salt_u8][tx_seq_be_u64]` — 9 bytes, where
//! `salt_u8 = (tx_seq % SALT_COUNT) as u8`. The salt prefix spreads writes
//! across `SALT_COUNT` tablet prefixes so an append-only, monotonic
//! `tx_sequence_number` doesn't funnel every write into a single trailing
//! tablet. Within a salt bucket rows are still sorted by tx_seq, so point
//! lookups via `multi_get` and per-shard range scans still work; a full
//! ascending scan has to fan out across all `SALT_COUNT` buckets and
//! merge client-side (see `scan_tx_seq_digest_stream`).
//!
//! `event_count` lets readers enumerate a transaction's event_seqs without
//! reading the tx row itself — used by unfiltered event listing to bound
//! the walk to exactly the events contributing to a page.

use anyhow::{Context, Result};
use bytes::Bytes;
use sui_types::digests::TransactionDigest;

pub const NAME: &str = "tx_seq_digest";

/// Number of salt prefix buckets. Power of 2 so `tx_seq % SALT_COUNT` stays cheap.
/// Writes spread across this many tablet prefixes to avoid the sequential-PK
/// hotspot from monotonic `tx_sequence_number`. Readers that need strictly
/// ascending iteration fan out across all `SALT_COUNT` shards and k-way merge.
/// 64 keeps the page-fill fan-out modest while leaving enough write headroom
/// to scale without a table rewrite.
pub const SALT_COUNT: u64 = 64;

pub mod col {
    /// Raw 32-byte TransactionDigest.
    pub const DIGEST: &str = "d";
    /// Big-endian u32 count of events emitted by this transaction.
    pub const EVENT_COUNT: &str = "e";
}

/// Row key: `[salt_u8][tx_seq_be_u64]`, where `salt_u8 = (tx_seq % SALT_COUNT) as u8`.
pub fn encode_key(tx_seq: u64) -> Vec<u8> {
    let mut k = Vec::with_capacity(9);
    k.push((tx_seq % SALT_COUNT) as u8);
    k.extend_from_slice(&tx_seq.to_be_bytes());
    k
}

/// Decode a salted row key, validating length and salt-byte consistency.
pub fn decode_key(key: &[u8]) -> Result<u64> {
    if key.len() != 9 {
        anyhow::bail!("tx_seq_digest key not 9 bytes (got {})", key.len());
    }
    let tx_seq = u64::from_be_bytes(key[1..].try_into().unwrap());
    let expected_salt = (tx_seq % SALT_COUNT) as u8;
    if key[0] != expected_salt {
        anyhow::bail!(
            "tx_seq_digest salt byte mismatch: key[0]={} expected={}",
            key[0],
            expected_salt
        );
    }
    Ok(tx_seq)
}

pub fn encode(digest: &TransactionDigest, event_count: u32) -> [(&'static str, Bytes); 2] {
    [
        (col::DIGEST, Bytes::from(digest.inner().to_vec())),
        (
            col::EVENT_COUNT,
            Bytes::copy_from_slice(&event_count.to_be_bytes()),
        ),
    ]
}

pub fn decode(cells: &[(Bytes, Bytes)]) -> Result<(TransactionDigest, u32)> {
    let mut digest: Option<TransactionDigest> = None;
    let mut event_count: u32 = 0;
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
        }
    }
    Ok((
        digest.context("tx_seq_digest missing digest column")?,
        event_count,
    ))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn encode_key_layout() {
        for tx_seq in [0u64, 1, 63, 64, 65, 1_000_000, u64::MAX] {
            let k = encode_key(tx_seq);
            assert_eq!(k.len(), 9, "key must be 9 bytes for tx_seq={tx_seq}");
            assert_eq!(
                k[0],
                (tx_seq % SALT_COUNT) as u8,
                "salt byte for tx_seq={tx_seq}"
            );
            assert_eq!(&k[1..], &tx_seq.to_be_bytes());
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
        let cells = encode(&digest, event_count);

        assert_eq!(cells[1].1.as_ref(), &event_count.to_be_bytes());

        let cells = cells
            .into_iter()
            .map(|(column, value)| (Bytes::from_static(column.as_bytes()), value))
            .collect::<Vec<_>>();
        let (decoded_digest, decoded_event_count) = decode(&cells).unwrap();
        assert_eq!(decoded_digest, digest);
        assert_eq!(decoded_event_count, event_count);
    }

    #[test]
    fn decode_key_rejects_wrong_length() {
        assert!(decode_key(&[0u8; 8]).is_err());
        assert!(decode_key(&[0u8; 10]).is_err());
        assert!(decode_key(&[]).is_err());
    }

    #[test]
    fn decode_key_rejects_inconsistent_salt() {
        let tx_seq: u64 = 1_000;
        let mut k = encode_key(tx_seq);
        k[0] = k[0].wrapping_add(1);
        assert!(decode_key(&k).is_err());
    }
}
