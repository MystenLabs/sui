// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

//! Watermarks table: stores per-pipeline watermarks indexed by pipeline name.

use anyhow::{Context, Result};
use bytes::Bytes;

use crate::PipelineWatermark;

pub mod col {
    pub const WATERMARK: &str = "w";
}

pub const NAME: &str = "watermarks";

pub fn encode_key(pipeline: &str) -> Vec<u8> {
    pipeline.as_bytes().to_vec()
}

pub fn encode(watermark: &PipelineWatermark) -> Result<[(&'static str, Bytes); 1]> {
    Ok([(col::WATERMARK, Bytes::from(bcs::to_bytes(watermark)?))])
}

pub fn decode(row: &[(Bytes, Bytes)]) -> Result<PipelineWatermark> {
    let (_, value) = row.first().context("empty row")?;
    Ok(bcs::from_bytes(value)?)
}
