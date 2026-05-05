// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

//! Watermarks table: stores per-pipeline watermarks indexed by pipeline name.

use anyhow::Result;
use anyhow::bail;
use bytes::Bytes;

use crate::Watermark;

pub mod col {
    pub const WATERMARK: &str = "w";
}

pub const NAME: &str = "watermark_alt";

pub fn encode_key(pipeline: &str) -> Vec<u8> {
    pipeline.as_bytes().to_vec()
}

pub fn encode(watermark: &Watermark) -> Result<[(&'static str, Bytes); 1]> {
    Ok([(col::WATERMARK, Bytes::from(bcs::to_bytes(watermark)?))])
}

pub fn decode(row: &[(Bytes, Bytes)]) -> Result<Watermark> {
    for (col, value) in row {
        if col.as_ref() == col::WATERMARK.as_bytes() {
            return Ok(bcs::from_bytes(value)?);
        }
    }
    bail!("`{}` column missing from watermark row", col::WATERMARK)
}
