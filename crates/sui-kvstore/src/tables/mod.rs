// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

//! Table schema definitions with encode/decode functions for BigTable.
//!
//! Each table module contains:
//! - `NAME`: Table name constant
//! - `col`: Column qualifier constants (if multi-column)
//! - `key()`: Row key encoding
//! - `encode()`: Create an Entry from typed data
//! - `decode()`: Parse typed data from row cells

use std::io::Write as _;

use bytes::Bytes;

use crate::bigtable::proto::bigtable::v2::mutate_rows_request::Entry;
use crate::bigtable::proto::bigtable::v2::mutation::SetCell;
use crate::bigtable::proto::bigtable::v2::{Mutation, mutation};

pub mod checkpoints;
pub mod checkpoints_by_digest;
pub mod epochs;
pub mod event_bitmap_index;
pub mod object_versions_by_checkpoint;
pub mod objects;
pub mod packages;
pub mod packages_by_checkpoint;
pub mod packages_by_id;
pub mod protocol_configs;
pub mod system_packages;
pub mod transaction_bitmap_index;
pub mod transactions;
pub mod tx_seq_digest;
pub mod watermarks;

/// Column family name used by all tables.
pub const FAMILY: &str = "sui";

/// Default column qualifier (empty string) used by single-column tables.
pub const DEFAULT_COLUMN: &str = "";

/// Row layout of a bitmap index table: its row-key format and how its cells are timestamped.
#[derive(
    Clone, Copy, Debug, PartialEq, Eq, serde::Deserialize, serde::Serialize, schemars::JsonSchema,
)]
#[serde(rename_all = "lowercase")]
pub enum BitmapSchema {
    /// The `transaction_bitmap_index` and `event_bitmap_index` tables. Row keys start with
    /// `v1#`, and a cell's timestamp is the timestamp of the last checkpoint that added a bit to
    /// the row. Checkpoints can share a timestamp, so when two writes of a row land out of order,
    /// the row can keep the older snapshot.
    V1,
    /// The `transaction_bitmap_index_v2` and `event_bitmap_index_v2` tables. Row keys have no
    /// version prefix, and a cell's timestamp is `2^42 + checkpoint` milliseconds, where
    /// `checkpoint` is the sequence number of the last checkpoint that added a bit to the row.
    V2,
}

impl BitmapSchema {
    /// Cell timestamp, in milliseconds, of a bitmap whose last bit came from `checkpoint`, which
    /// has timestamp `checkpoint_timestamp_ms`.
    pub(crate) fn cell_timestamp_ms(self, checkpoint: u64, checkpoint_timestamp_ms: u64) -> u64 {
        match self {
            Self::V1 => checkpoint_timestamp_ms,
            Self::V2 => checkpoint_cell_timestamp_ms(checkpoint),
        }
    }
}

/// Row layout of the objects table: which table holds object versions and how its row keys
/// order them.
#[derive(
    Clone,
    Copy,
    Debug,
    Default,
    PartialEq,
    Eq,
    serde::Deserialize,
    serde::Serialize,
    schemars::JsonSchema,
)]
#[serde(rename_all = "lowercase")]
pub enum ObjectSchema {
    /// The `objects` table, written by the `kvstore_objects` pipeline. Row keys are
    /// `object_id || version`, so an object's versions sort oldest first. Deletions and wraps are
    /// recorded only where the alpha `kvstore_object_tombstones` pipeline has run; elsewhere a
    /// removed object resolves to its last live version.
    #[default]
    V1,
    /// The `objects_v2` table, written by the `kvstore_objects_v2` pipeline. Row keys are
    /// `object_id || (u64::MAX - version)`, so an object's versions sort newest first, and the
    /// table also holds a tombstone at every version that deleted or wrapped an object.
    V2,
}

/// Row-key prefix of [`BitmapSchema::V1`] tables.
const V1_BITMAP_ROW_KEY_PREFIX: &[u8] = b"v1#";

/// Shared row-key encoder for bitmap indexes.
///
/// Format: `{prefix}{dimension_key}#{bucket_id}`, where the prefix is `v1#` for
/// [`BitmapSchema::V1`] and empty for [`BitmapSchema::V2`], and `bucket_id` is zero-padded to
/// the table's configured width so bucket rows for a dimension sort lexicographically by bucket.
pub(crate) fn encode_bitmap_row_key_into(
    out: &mut Vec<u8>,
    schema: BitmapSchema,
    bucket_id_width: usize,
    dimension_key: &[u8],
    bucket_id: u64,
) {
    encode_bitmap_row_key_with_dimension(
        out,
        schema,
        bucket_id_width,
        dimension_key.len(),
        bucket_id,
        |out| {
            out.extend_from_slice(dimension_key);
        },
    );
}

pub(crate) fn encode_bitmap_row_key_parts_into(
    out: &mut Vec<u8>,
    schema: BitmapSchema,
    bucket_id_width: usize,
    dimension_tag: u8,
    dimension_value: &[u8],
    bucket_id: u64,
) {
    encode_bitmap_row_key_with_dimension(
        out,
        schema,
        bucket_id_width,
        1 + dimension_value.len(),
        bucket_id,
        |out| {
            out.push(dimension_tag);
            out.extend_from_slice(dimension_value);
        },
    );
}

fn encode_bitmap_row_key_with_dimension(
    out: &mut Vec<u8>,
    schema: BitmapSchema,
    bucket_id_width: usize,
    dimension_len: usize,
    bucket_id: u64,
    write_dimension: impl FnOnce(&mut Vec<u8>),
) {
    out.clear();
    // u64 buckets are at most 20 decimal digits.
    out.reserve(V1_BITMAP_ROW_KEY_PREFIX.len() + dimension_len + 1 + bucket_id_width.max(20));
    if schema == BitmapSchema::V1 {
        out.extend_from_slice(V1_BITMAP_ROW_KEY_PREFIX);
    }
    write_dimension(out);
    write!(out, "#{:0width$}", bucket_id, width = bucket_id_width)
        .expect("writing to Vec cannot fail");
}

/// Start of the cell timestamps written by [`checkpoint_cell_timestamp_ms`], in milliseconds.
/// 2^42 ms is past every checkpoint timestamp and the server's time until the year 2109, so
/// these cells sort above any cell timestamped with a checkpoint's time or the server's time.
const CHECKPOINT_CELL_TIMESTAMP_BASE_MS: u64 = 1 << 42;

/// Cell timestamp, in milliseconds, for a value that reflects every checkpoint through
/// `checkpoint`.
///
/// Readers take a column's newest cell, and several writers index the same checkpoints. A cell
/// rewritten with a value that grows as checkpoints are indexed, like a bitmap bucket, must
/// therefore order its versions by the checkpoint each reflects. Checkpoint timestamps cannot:
/// checkpoints split from one consensus commit share a timestamp, and of two writes at one
/// timestamp the later one wins, whichever checkpoint it reflects.
pub(crate) fn checkpoint_cell_timestamp_ms(checkpoint: u64) -> u64 {
    CHECKPOINT_CELL_TIMESTAMP_BASE_MS + checkpoint
}

/// Build an Entry from cells. Accepts any iterator to avoid intermediate allocations.
pub fn make_entry(
    row_key: impl Into<Bytes>,
    cells: impl IntoIterator<Item = (&'static str, Bytes)>,
    timestamp_ms: Option<u64>,
) -> Entry {
    let timestamp_micros = timestamp_ms
        .map(|ms| ms.checked_mul(1000).expect("timestamp overflow") as i64)
        // default to -1 for current Bigtable server time
        .unwrap_or(-1);

    Entry {
        row_key: row_key.into(),
        mutations: cells
            .into_iter()
            .map(|(col, val)| Mutation {
                mutation: Some(mutation::Mutation::SetCell(SetCell {
                    family_name: FAMILY.to_string(),
                    column_qualifier: Bytes::from(col),
                    timestamp_micros,
                    value: val,
                })),
            })
            .collect(),
        idempotency: None,
    }
}
