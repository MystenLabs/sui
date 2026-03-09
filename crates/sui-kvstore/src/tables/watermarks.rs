// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

//! Watermarks table: stores per-pipeline watermarks indexed by pipeline name.

use anyhow::{Context, Result};
use bytes::Bytes;

use crate::Watermark;

pub mod col {
    pub const WATERMARK: &str = "w";
}

/// Pipeline watermarks are now stored in the `watermark_alt` table alongside
/// the legacy `[0]` row, so this constant points there.
pub const NAME: &str = super::watermark_alt_legacy::NAME;

pub fn encode_key(pipeline: &str) -> Vec<u8> {
    pipeline.as_bytes().to_vec()
}

pub fn encode(watermark: &Watermark) -> Result<[(&'static str, Bytes); 1]> {
    Ok([(col::WATERMARK, Bytes::from(bcs::to_bytes(watermark)?))])
}

pub fn decode(row: &[(Bytes, Bytes)]) -> Result<Watermark> {
    let (_, value) = row.first().context("empty row")?;
    Ok(bcs::from_bytes(value)?)
}
