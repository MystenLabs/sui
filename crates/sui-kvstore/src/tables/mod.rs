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

pub mod checkpoint_bitmap_index;
pub mod checkpoints;
pub mod checkpoints_by_digest;
pub mod epochs;
pub mod event_bitmap_index;
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

/// Shared row-key encoder for bitmap indexes.
///
/// Format: `v{version}#{dimension_key}#{bucket_id}`, where `bucket_id` is
/// zero-padded to the table's configured width so bucket rows for a dimension
/// sort lexicographically by bucket.
pub(crate) fn encode_bitmap_row_key_into(
    out: &mut Vec<u8>,
    version: u32,
    bucket_id_width: usize,
    dimension_key: &[u8],
    bucket_id: u64,
) {
    encode_bitmap_row_key_with_dimension(
        out,
        version,
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
    version: u32,
    bucket_id_width: usize,
    dimension_tag: u8,
    dimension_value: &[u8],
    bucket_id: u64,
) {
    encode_bitmap_row_key_with_dimension(
        out,
        version,
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
    version: u32,
    bucket_id_width: usize,
    dimension_len: usize,
    bucket_id: u64,
    write_dimension: impl FnOnce(&mut Vec<u8>),
) {
    out.clear();
    // u32 versions are at most 10 decimal digits; u64 buckets are at most 20.
    out.reserve(2 + 10 + dimension_len + 1 + bucket_id_width.max(20));
    write!(out, "v{version}#").expect("writing to Vec cannot fail");
    write_dimension(out);
    write!(out, "#{:0width$}", bucket_id, width = bucket_id_width)
        .expect("writing to Vec cannot fail");
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
