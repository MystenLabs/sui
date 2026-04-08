// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

//! Watermarks table: stores per-pipeline watermarks indexed by pipeline name.
//!
//! A row in the `v1` schema has one cell per [`WatermarkV1`] field (u64 big-endian), plus a
//! schema-version tag cell and an optional BCS-encoded [`WatermarkV0`] cell:
//! - `ehi` / `chi` / `th` / `tmhi` / `rl` / `ph` / `ptm`: per-field u64 BE cells. `chi`
//!   (checkpoint_hi_inclusive) is absent when the committer has not observed a checkpoint
//!   yet (`Option<u64>::None`).
//! - `v`: u64 BE schema version. Rows in the current schema carry `v = SCHEMA_V1`;
//!   `create_pipeline_watermark_if_absent` keys off this cell to detect row existence,
//!   and a future format migration can bump it to branch read behavior.
//! - `w` (v0): BCS-encoded [`WatermarkV0`], kept in sync alongside the per-field cells so
//!   existing BCS consumers keep working.

use anyhow::Context;
use anyhow::Result;
use anyhow::bail;
use bytes::Bytes;

use crate::WatermarkV0;
use crate::WatermarkV1;

pub mod col {
    /// BCS-encoded v0 watermark column.
    pub const WATERMARK_V0: &str = "w";
    /// Schema version tag for the `v1` per-field cell layout.
    pub const SCHEMA_VERSION: &str = "v";
    /// v1 per-field cells (all u64 big-endian).
    pub const EPOCH_HI: &str = "ehi";
    pub const CHECKPOINT_HI: &str = "chi";
    pub const TX_HI: &str = "th";
    pub const TIMESTAMP_MS_HI: &str = "tmhi";
    pub const READER_LO: &str = "rl";
    pub const PRUNER_HI: &str = "ph";
    pub const PRUNER_TIMESTAMP_MS: &str = "ptm";
}

/// Current schema version written into the `v` cell.
pub const SCHEMA_V1: u64 = 1;

pub const NAME: &str = "watermark_alt";

pub fn encode_key(pipeline: &str) -> Vec<u8> {
    pipeline.as_bytes().to_vec()
}

/// Single `(w, BCS)` cell. Used by tests that seed v0-format data.
pub fn encode_v0(v0: &WatermarkV0) -> Result<(&'static str, Bytes)> {
    Ok((col::WATERMARK_V0, Bytes::from(bcs::to_bytes(v0)?)))
}

fn decode_u64_be(val: &Bytes, col_name: &str) -> Result<u64> {
    if val.len() != 8 {
        bail!(
            "`{}` column has unexpected length {} (expected 8)",
            col_name,
            val.len()
        );
    }
    let mut buf = [0u8; 8];
    buf.copy_from_slice(val);
    Ok(u64::from_be_bytes(buf))
}

/// Strict read of the v1 cells. Returns `Ok(None)` if the schema-version `v` cell is absent
/// (v0-only row or row does not exist). Bails on unknown schema versions, and on missing
/// required field cells when `v` is present (which would indicate data corruption since all
/// field cells are written atomically with `v`). `chi` is the only optional field.
pub fn decode_v1(row: &[(Bytes, Bytes)]) -> Result<Option<WatermarkV1>> {
    let mut schema_version: Option<&Bytes> = None;
    let mut epoch_hi: Option<&Bytes> = None;
    let mut checkpoint_hi: Option<&Bytes> = None;
    let mut tx_hi: Option<&Bytes> = None;
    let mut timestamp_ms_hi: Option<&Bytes> = None;
    let mut reader_lo: Option<&Bytes> = None;
    let mut pruner_hi: Option<&Bytes> = None;
    let mut pruner_timestamp_ms: Option<&Bytes> = None;
    for (col, val) in row {
        match col.as_ref() {
            b if b == col::SCHEMA_VERSION.as_bytes() => schema_version = Some(val),
            b if b == col::EPOCH_HI.as_bytes() => epoch_hi = Some(val),
            b if b == col::CHECKPOINT_HI.as_bytes() => checkpoint_hi = Some(val),
            b if b == col::TX_HI.as_bytes() => tx_hi = Some(val),
            b if b == col::TIMESTAMP_MS_HI.as_bytes() => timestamp_ms_hi = Some(val),
            b if b == col::READER_LO.as_bytes() => reader_lo = Some(val),
            b if b == col::PRUNER_HI.as_bytes() => pruner_hi = Some(val),
            b if b == col::PRUNER_TIMESTAMP_MS.as_bytes() => pruner_timestamp_ms = Some(val),
            _ => {}
        }
    }
    let Some(v) = schema_version else {
        return Ok(None);
    };
    let schema = decode_u64_be(v, col::SCHEMA_VERSION)?;
    if schema != SCHEMA_V1 {
        bail!("unknown watermark schema version {}", schema);
    }
    let missing = |name: &str| anyhow::anyhow!("`{}` column missing from v1 watermark row", name);
    let decode_required = |cell: Option<&Bytes>, name: &'static str| -> Result<u64> {
        decode_u64_be(cell.ok_or_else(|| missing(name))?, name)
    };
    Ok(Some(WatermarkV1 {
        epoch_hi_inclusive: decode_required(epoch_hi, col::EPOCH_HI)?,
        checkpoint_hi_inclusive: checkpoint_hi
            .map(|v| decode_u64_be(v, col::CHECKPOINT_HI))
            .transpose()?,
        tx_hi: decode_required(tx_hi, col::TX_HI)?,
        timestamp_ms_hi_inclusive: decode_required(timestamp_ms_hi, col::TIMESTAMP_MS_HI)?,
        reader_lo: decode_required(reader_lo, col::READER_LO)?,
        pruner_hi: decode_required(pruner_hi, col::PRUNER_HI)?,
        pruner_timestamp_ms: decode_required(pruner_timestamp_ms, col::PRUNER_TIMESTAMP_MS)?,
    }))
}

/// Reads only the v0 `w` cell. Returns `Ok(None)` if absent. Used **only** by
/// `init_watermark` to bootstrap the v1 format from a v0-only row.
pub fn decode_v0(row: &[(Bytes, Bytes)]) -> Result<Option<WatermarkV0>> {
    for (col, val) in row {
        if col.as_ref() == col::WATERMARK_V0.as_bytes() {
            return Ok(Some(bcs::from_bytes(val).context(
                "failed to deserialize BCS v0 watermark from `w` column",
            )?));
        }
    }
    Ok(None)
}
