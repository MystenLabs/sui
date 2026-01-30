// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

//! Epochs table: stores epoch info indexed by epoch ID.

use anyhow::{Context, Result};
use bytes::Bytes;
use sui_types::committee::EpochId;
use sui_types::storage::EpochInfo;

use crate::tables::DEFAULT_COLUMN;
use crate::{EpochEndData, EpochStartData};

pub const NAME: &str = "epochs";

pub mod col {
    pub const START: &str = "start";
    pub const END: &str = "end";
}

pub fn encode_key(epoch_id: EpochId) -> Vec<u8> {
    epoch_id.to_be_bytes().to_vec()
}

pub fn encode_key_upper_bound() -> Bytes {
    Bytes::from(u64::MAX.to_be_bytes().to_vec())
}

/// Encode full epoch info (legacy format).
pub fn encode(epoch: &EpochInfo) -> Result<[(&'static str, Bytes); 1]> {
    Ok([(DEFAULT_COLUMN, Bytes::from(bcs::to_bytes(epoch)?))])
}

/// Encode epoch start data to the START column.
pub fn encode_start(data: &EpochStartData) -> Result<[(&'static str, Bytes); 1]> {
    Ok([(col::START, Bytes::from(bcs::to_bytes(data)?))])
}

/// Encode epoch end data to the END column.
pub fn encode_end(data: &EpochEndData) -> Result<[(&'static str, Bytes); 1]> {
    Ok([(col::END, Bytes::from(bcs::to_bytes(data)?))])
}

pub fn decode(row: &[(Bytes, Bytes)]) -> Result<EpochInfo> {
    let (_, value) = row.first().context("empty row")?;
    Ok(bcs::from_bytes(value)?)
}
