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

use bytes::Bytes;

use crate::bigtable::proto::bigtable::v2::mutate_rows_request::Entry;
use crate::bigtable::proto::bigtable::v2::mutation::SetCell;
use crate::bigtable::proto::bigtable::v2::{Mutation, mutation};

pub mod checkpoints;
pub mod checkpoints_by_digest;
pub mod epochs;
pub mod object_types;
pub mod objects;
pub mod transactions;
pub mod watermarks;

/// Column family name used by all tables.
pub const FAMILY: &str = "sui";

/// Default column qualifier (empty string) used by single-column tables.
pub const DEFAULT_COLUMN: &str = "";

/// Deprecated: Legacy watermark tables, replaced by per-pipeline watermarks table.
pub mod watermark_legacy {
    /// Stores checkpoint numbers as row keys (scan to find latest).
    pub const NAME: &str = "watermark";
}

/// Deprecated: Legacy watermark table with single row.
pub mod watermark_alt_legacy {
    /// Stores checkpoint number at row key \[0\] (single read).
    pub const NAME: &str = "watermark_alt";
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
