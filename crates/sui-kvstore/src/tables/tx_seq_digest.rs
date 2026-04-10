// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

//! Direct mapping from `tx_sequence_number → (TransactionDigest, checkpoint_seq, event_count)`.
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
/// 16 balances write spread against the per-shard scan overhead the fan-out
/// reader pays on each request.
pub const SALT_COUNT: u64 = 16;

pub mod col {
    /// Raw 32-byte TransactionDigest.
    pub const DIGEST: &str = "d";
    /// BCS-encoded u64 checkpoint sequence number.
    pub const CHECKPOINT_SEQ: &str = "c";
    /// BCS-encoded u32 count of events emitted by this transaction.
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

pub fn encode(
    digest: &TransactionDigest,
    checkpoint_seq: u64,
    event_count: u32,
) -> [(&'static str, Bytes); 3] {
    [
        (col::DIGEST, Bytes::from(digest.inner().to_vec())),
        (
            col::CHECKPOINT_SEQ,
            Bytes::from(bcs::to_bytes(&checkpoint_seq).unwrap()),
        ),
        (
            col::EVENT_COUNT,
            Bytes::from(bcs::to_bytes(&event_count).unwrap()),
        ),
    ]
}

pub fn decode(cells: &[(Bytes, Bytes)]) -> Result<(TransactionDigest, u64, u32)> {
    let mut digest: Option<TransactionDigest> = None;
    let mut cp_seq: Option<u64> = None;
    let mut event_count: u32 = 0;
    for (column, value) in cells {
        if column.as_ref() == col::DIGEST.as_bytes() {
            let bytes: [u8; 32] = value
                .as_ref()
                .try_into()
                .context("tx_seq_digest digest not 32 bytes")?;
            digest = Some(TransactionDigest::from(bytes));
        } else if column.as_ref() == col::CHECKPOINT_SEQ.as_bytes() {
            cp_seq = Some(bcs::from_bytes(value).context("invalid checkpoint_seq BCS")?);
        } else if column.as_ref() == col::EVENT_COUNT.as_bytes() {
            event_count = bcs::from_bytes(value).context("invalid event_count BCS")?;
        }
    }
    Ok((
        digest.context("tx_seq_digest missing digest column")?,
        cp_seq.context("tx_seq_digest missing checkpoint_seq column")?,
        event_count,
    ))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn encode_key_layout() {
        for tx_seq in [0u64, 1, 15, 16, 17, 1_000_000, u64::MAX] {
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
        for tx_seq in [0u64, 1, 15, 16, 17, 1_000_000, u64::MAX] {
            let k = encode_key(tx_seq);
            let got = decode_key(&k).expect("decode must accept freshly encoded key");
            assert_eq!(got, tx_seq);
        }
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

    #[test]
    fn consecutive_seqs_always_change_salt() {
        for tx_seq in 0u64..1_000 {
            let a = encode_key(tx_seq)[0];
            let b = encode_key(tx_seq + 1)[0];
            assert_ne!(
                a,
                b,
                "consecutive seqs {tx_seq} and {} collided",
                tx_seq + 1
            );
        }
    }

    #[test]
    fn ten_k_sequential_seqs_cover_all_salts() {
        let start: u64 = 1_000_000;
        let mut seen = std::collections::HashSet::new();
        for tx_seq in start..start + 10_000 {
            seen.insert(encode_key(tx_seq)[0]);
        }
        assert_eq!(
            seen.len(),
            SALT_COUNT as usize,
            "expected every salt bucket to appear in 10k sequential seqs"
        );
    }
}
