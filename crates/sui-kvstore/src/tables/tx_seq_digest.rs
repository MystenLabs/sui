// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

//! Direct mapping from `tx_sequence_number → (TransactionDigest, event_count, tx_offset, checkpoint_number)`.
//!
//! Row key layout: `v{version}#{bucket_prefix:016x}#{bucket_id:020}#{tx_seq:020}`.
//! `bucket_prefix` is a deterministic bit-reversal of `bucket_id`, spreading
//! active write buckets across BigTable's keyspace. Readers scan bucket-local
//! row ranges in logical bucket order.
//!
//! `event_count` lets readers enumerate a transaction's event_seqs without
//! reading the tx row itself — used by unfiltered event listing to discover
//! exactly the events contributing to a page.
//!
//! `tx_offset` is the transaction's zero-based position within its checkpoint.
//! It lets readers report a `(checkpoint, tx_offset)` coordinate without an
//! extra `checkpoints` lookup to recover the checkpoint's first tx_seq. The
//! column is assumed always present (the table is backfilled before serving),
//! so `decode` treats a missing `tx_offset` as an error.

use anyhow::bail;
use anyhow::{Context, Result};
use bytes::Bytes;
use integer_encoding::VarInt;
use std::ops::Range;
use sui_types::digests::TransactionDigest;
use sui_types::messages_checkpoint::CheckpointSequenceNumber;

pub const NAME: &str = "tx_seq_digest";
pub const SCHEMA_VERSION: u32 = 1;
/// Number of tx_sequence_numbers per forward-scan bucket. Tied to
/// SCHEMA_VERSION; changing it requires a version bump and backfill.
pub const FORWARD_BUCKET_SIZE: u64 = 1_000;
const FORWARD_KEY_NUMBER_WIDTH: usize = 20;
const FORWARD_BUCKET_PREFIX_WIDTH: usize = 16;

pub mod col {
    /// Raw 32-byte TransactionDigest.
    pub const DIGEST: &str = "d";
    /// Big-endian u32 count of events emitted by this transaction.
    pub const EVENT_COUNT: &str = "e";
    /// Varint-encoded zero-based position of this transaction within its checkpoint.
    pub const TX_OFFSET: &str = "o";
    /// Varint-encoded checkpoint sequence number containing this transaction.
    pub const CHECKPOINT_NUMBER: &str = "c";
}

pub fn encode_key(tx_seq: u64) -> Vec<u8> {
    encode_key_for_version(SCHEMA_VERSION, tx_seq)
}

pub fn forward_bucket_id(tx_seq: u64) -> u64 {
    tx_seq / FORWARD_BUCKET_SIZE
}

pub fn distributed_bucket_prefix(bucket_id: u64) -> u64 {
    bucket_id.reverse_bits()
}

fn encode_key_for_version(version: u32, tx_seq: u64) -> Vec<u8> {
    let bucket_id = forward_bucket_id(tx_seq);
    format!(
        "v{}#{:0prefix_width$x}#{:0number_width$}#{:0number_width$}",
        version,
        distributed_bucket_prefix(bucket_id),
        bucket_id,
        tx_seq,
        prefix_width = FORWARD_BUCKET_PREFIX_WIDTH,
        number_width = FORWARD_KEY_NUMBER_WIDTH,
    )
    .into_bytes()
}

pub fn clamp_range_to_limit(range: Range<u64>, descending: bool, limit: usize) -> Range<u64> {
    if range.is_empty() || limit == 0 {
        return range.start..range.start;
    }

    let limit = limit as u64;
    if descending {
        range.end.saturating_sub(limit).max(range.start)..range.end
    } else {
        range.start..range.start.saturating_add(limit).min(range.end)
    }
}

pub fn split_range_by_bucket(range: Range<u64>, descending: bool) -> Vec<Range<u64>> {
    if range.is_empty() {
        return Vec::new();
    }

    let mut ranges = Vec::new();
    let mut start = range.start;
    while start < range.end {
        let next_bucket_start = forward_bucket_id(start)
            .saturating_add(1)
            .saturating_mul(FORWARD_BUCKET_SIZE);
        let end = next_bucket_start.min(range.end);
        ranges.push(start..end);
        start = end;
    }

    if descending {
        ranges.reverse();
    }
    ranges
}

pub fn decode_key(key: &[u8]) -> Result<u64> {
    let key = std::str::from_utf8(key).context("tx_seq_digest key is not utf8")?;
    let mut parts = key.split('#');
    let version = parts.next().context("tx_seq_digest key missing version")?;
    let bucket_prefix = parts
        .next()
        .context("tx_seq_digest key missing bucket prefix")?;
    let bucket = parts.next().context("tx_seq_digest key missing bucket")?;
    let tx_seq = parts.next().context("tx_seq_digest key missing tx_seq")?;
    if parts.next().is_some() {
        bail!("tx_seq_digest key has too many parts");
    }
    if !version.starts_with('v') || version.len() == 1 {
        bail!("tx_seq_digest key version missing v-prefix");
    }
    let version: u32 = version[1..]
        .parse()
        .context("tx_seq_digest key version is not a u32")?;
    if version != SCHEMA_VERSION {
        bail!(
            "tx_seq_digest key version {version} does not match current version {SCHEMA_VERSION}"
        );
    }
    if bucket_prefix.len() != FORWARD_BUCKET_PREFIX_WIDTH {
        bail!("tx_seq_digest key bucket prefix width is invalid");
    }
    let bucket_prefix = u64::from_str_radix(bucket_prefix, 16)
        .context("tx_seq_digest key bucket prefix is not hex u64")?;
    if bucket.len() != FORWARD_KEY_NUMBER_WIDTH || tx_seq.len() != FORWARD_KEY_NUMBER_WIDTH {
        bail!("tx_seq_digest key number width is invalid");
    }
    let bucket_id: u64 = bucket
        .parse()
        .context("tx_seq_digest key bucket is not a u64")?;
    let tx_seq: u64 = tx_seq
        .parse()
        .context("tx_seq_digest key tx_seq is not a u64")?;
    if bucket_id != forward_bucket_id(tx_seq) {
        bail!("tx_seq_digest key bucket does not match tx_seq");
    }
    if bucket_prefix != distributed_bucket_prefix(bucket_id) {
        bail!("tx_seq_digest key bucket prefix does not match bucket");
    }
    Ok(tx_seq)
}

pub fn encode(
    digest: &TransactionDigest,
    event_count: u32,
    tx_offset: u32,
    checkpoint_number: CheckpointSequenceNumber,
) -> [(&'static str, Bytes); 4] {
    [
        (col::DIGEST, Bytes::from(digest.inner().to_vec())),
        (
            col::EVENT_COUNT,
            Bytes::copy_from_slice(&event_count.to_be_bytes()),
        ),
        (col::TX_OFFSET, Bytes::from(tx_offset.encode_var_vec())),
        (
            col::CHECKPOINT_NUMBER,
            Bytes::from(checkpoint_number.encode_var_vec()),
        ),
    ]
}

pub fn decode(
    cells: &[(Bytes, Bytes)],
) -> Result<(TransactionDigest, u32, u32, CheckpointSequenceNumber)> {
    let mut digest: Option<TransactionDigest> = None;
    let mut event_count: u32 = 0;
    let mut tx_offset = None;
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
        } else if column.as_ref() == col::TX_OFFSET.as_bytes() {
            tx_offset = Some(decode_canonical_varint::<u32>(value, "tx_offset")?);
        } else if column.as_ref() == col::CHECKPOINT_NUMBER.as_bytes() {
            checkpoint_number = Some(decode_checkpoint_number_value(value)?);
        }
    }
    Ok((
        digest.context("tx_seq_digest missing digest column")?,
        event_count,
        tx_offset.context("tx_seq_digest missing tx_offset column")?,
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
    decode_canonical_varint::<u64>(value, "checkpoint_number")
}

/// Decode a canonical varint cell value, rejecting trailing or non-minimal
/// bytes so each value has exactly one valid encoding.
fn decode_canonical_varint<T: VarInt>(value: &Bytes, field: &str) -> Result<T> {
    let (decoded, bytes_read) = T::decode_var(value.as_ref())
        .with_context(|| format!("tx_seq_digest {field} is not a valid varint"))?;
    if bytes_read != value.len() {
        bail!("tx_seq_digest {field} has trailing bytes");
    }
    if decoded.required_space() != bytes_read {
        bail!("tx_seq_digest {field} is not canonical");
    }
    Ok(decoded)
}

#[cfg(test)]
mod tests {
    use super::*;

    fn expected_key(tx_seq: u64) -> Vec<u8> {
        let bucket_id = forward_bucket_id(tx_seq);
        format!(
            "v1#{:016x}#{:020}#{:020}",
            distributed_bucket_prefix(bucket_id),
            bucket_id,
            tx_seq,
        )
        .into_bytes()
    }

    #[test]
    fn encode_key_layout() {
        assert_eq!(encode_key(0), expected_key(0));
        assert_eq!(encode_key(999), expected_key(999));
        assert_eq!(encode_key(1_000), expected_key(1_000));
        assert_eq!(encode_key(1_001), expected_key(1_001));
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
    fn encode_key_preserves_tx_order_within_bucket() {
        let tx_seqs = [0u64, 1, 998, 999];
        for pair in tx_seqs.windows(2) {
            assert!(
                encode_key(pair[0]) < encode_key(pair[1]),
                "key order must match tx_seq order within a bucket for {pair:?}"
            );
        }
    }

    #[test]
    fn forward_bucket_boundaries() {
        assert_eq!(forward_bucket_id(0), 0);
        assert_eq!(forward_bucket_id(999), 0);
        assert_eq!(forward_bucket_id(1_000), 1);
        assert_eq!(forward_bucket_id(1_001), 1);
    }

    #[test]
    fn distributed_bucket_prefix_distributes_adjacent_buckets() {
        assert_ne!(distributed_bucket_prefix(0), distributed_bucket_prefix(1));
        assert_ne!(encode_key(999)[..20], encode_key(1_000)[..20]);
    }

    #[test]
    fn split_range_by_bucket_respects_direction() {
        assert_eq!(
            split_range_by_bucket(995..2_005, false),
            vec![995..1_000, 1_000..2_000, 2_000..2_005]
        );
        assert_eq!(
            split_range_by_bucket(995..2_005, true),
            vec![2_000..2_005, 1_000..2_000, 995..1_000]
        );
    }

    #[test]
    fn clamp_range_to_limit_respects_direction() {
        assert_eq!(clamp_range_to_limit(10..20, false, 4), 10..14);
        assert_eq!(clamp_range_to_limit(10..20, true, 4), 16..20);
        assert_eq!(clamp_range_to_limit(10..20, false, 100), 10..20);
        assert_eq!(clamp_range_to_limit(10..20, true, 100), 10..20);
    }

    #[test]
    fn encode_decode_row_round_trip() {
        let digest = TransactionDigest::new([7; 32]);
        let event_count = 123_456;
        let tx_offset = 42;
        let checkpoint_number = 300_000_000;
        let cells = encode(&digest, event_count, tx_offset, checkpoint_number);

        assert_eq!(cells[1].1.as_ref(), &event_count.to_be_bytes());
        assert_eq!(cells[2].1.as_ref(), tx_offset.encode_var_vec().as_slice());
        assert_eq!(cells[3].1.len(), 5);

        let cells = cells
            .into_iter()
            .map(|(column, value)| (Bytes::from_static(column.as_bytes()), value))
            .collect::<Vec<_>>();
        let (decoded_digest, decoded_event_count, decoded_tx_offset, decoded_checkpoint_number) =
            decode(&cells).unwrap();
        assert_eq!(decoded_digest, digest);
        assert_eq!(decoded_event_count, event_count);
        assert_eq!(decoded_tx_offset, tx_offset);
        assert_eq!(decoded_checkpoint_number, checkpoint_number);
        assert_eq!(decode_checkpoint_number(&cells).unwrap(), checkpoint_number);
    }

    /// The table is backfilled before serving, so a row without the
    /// `tx_offset` column is treated as corruption rather than defaulting.
    #[test]
    fn decode_rejects_missing_tx_offset() {
        let digest = TransactionDigest::new([3; 32]);
        let checkpoint_number = 7;
        let cells = vec![
            (
                Bytes::from_static(col::DIGEST.as_bytes()),
                Bytes::from(digest.inner().to_vec()),
            ),
            (
                Bytes::from_static(col::EVENT_COUNT.as_bytes()),
                Bytes::copy_from_slice(&5u32.to_be_bytes()),
            ),
            (
                Bytes::from_static(col::CHECKPOINT_NUMBER.as_bytes()),
                Bytes::from(checkpoint_number.encode_var_vec()),
            ),
        ];
        assert!(decode(&cells).is_err());
    }

    #[test]
    fn decode_key_rejects_invalid_keys() {
        assert!(decode_key(&[0u8; 7]).is_err());
        assert!(decode_key(&[0u8; 9]).is_err());
        assert!(decode_key(&[]).is_err());
        assert!(decode_key(b"v2#00000000000000000000#00000000000000000000").is_err());
        assert!(decode_key(b"v1#00000000000000000001#00000000000000000999").is_err());
        assert!(
            decode_key(b"v1#0000000000000000#00000000000000000001#00000000000000001000").is_err()
        );
    }
}
