// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

//! Watermarks table: stores per-pipeline watermarks indexed by pipeline name.

use anyhow::{Context, Result};
use bytes::Bytes;

use crate::Watermark;

pub mod col {
    pub const WATERMARK: &str = "w";
    /// `checkpoint_hi_inclusive` of the commit that first put a transaction
    /// into the currently-active bucket for a bitmap-index pipeline. Absent
    /// on non-bitmap pipelines and on bitmap pipelines that haven't yet
    /// transitioned through a bucket post-deploy.
    pub const BUCKET_START_CP: &str = "b";
}

pub const NAME: &str = "watermark_alt";

pub fn encode_key(pipeline: &str) -> Vec<u8> {
    pipeline.as_bytes().to_vec()
}

/// Build the cells to write for this watermark row. Bitmap-index pipelines
/// pass `Some(bucket_start_cp)` to persist the additional tracking column;
/// all other pipelines pass `None` and only the watermark cell is written.
pub fn encode(
    watermark: &Watermark,
    bucket_start_cp: Option<u64>,
) -> Result<Vec<(&'static str, Bytes)>> {
    let mut cells = Vec::with_capacity(2);
    cells.push((col::WATERMARK, Bytes::from(bcs::to_bytes(watermark)?)));
    if let Some(cp) = bucket_start_cp {
        cells.push((col::BUCKET_START_CP, Bytes::from(bcs::to_bytes(&cp)?)));
    }
    Ok(cells)
}

pub fn decode(row: &[(Bytes, Bytes)]) -> Result<Watermark> {
    for (col, value) in row {
        if col.as_ref() == col::WATERMARK.as_bytes() {
            return Ok(bcs::from_bytes(value)?);
        }
    }
    anyhow::bail!("watermark row missing '{}' column", col::WATERMARK)
}

/// Extract `bucket_start_cp` from a row, if present.
pub fn decode_bucket_start_cp(row: &[(Bytes, Bytes)]) -> Result<Option<u64>> {
    for (col, value) in row {
        if col.as_ref() == col::BUCKET_START_CP.as_bytes() {
            return Ok(Some(
                bcs::from_bytes(value).context("invalid bucket_start_cp BCS")?,
            ));
        }
    }
    Ok(None)
}
