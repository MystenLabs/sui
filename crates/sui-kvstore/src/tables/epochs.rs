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

/// Decode epoch info from row cells.
/// Prefers new format (START + optional END columns), falls back to legacy (DEFAULT_COLUMN).
pub fn decode(row: &[(Bytes, Bytes)]) -> Result<EpochInfo> {
    let mut epoch_info_legacy: Option<EpochInfo> = None;
    let mut epoch_start: Option<EpochStartData> = None;
    let mut epoch_end: Option<EpochEndData> = None;

    for (column, value) in row {
        match column.as_ref() {
            b"" => epoch_info_legacy = Some(bcs::from_bytes(value)?),
            b"start" => epoch_start = Some(bcs::from_bytes(value)?),
            b"end" => epoch_end = Some(bcs::from_bytes(value)?),
            _ => {}
        }
    }

    // Prefer START+END columns, fallback to legacy full EpochInfo
    match epoch_start {
        Some(start) => Ok(EpochInfo {
            epoch: start.epoch,
            protocol_version: start.protocol_version,
            start_timestamp_ms: start.start_timestamp_ms,
            start_checkpoint: start.start_checkpoint,
            reference_gas_price: start.reference_gas_price,
            system_state: start.system_state,
            end_timestamp_ms: epoch_end.as_ref().and_then(|e| e.end_timestamp_ms),
            end_checkpoint: epoch_end.as_ref().and_then(|e| e.end_checkpoint),
        }),
        None => epoch_info_legacy.context("epoch row missing both START and DEFAULT_COLUMN"),
    }
}
