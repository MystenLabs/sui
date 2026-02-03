// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

//! Epochs table: stores epoch info indexed by epoch ID.

use anyhow::Result;
use bytes::Bytes;
use sui_types::committee::EpochId;
use sui_types::storage::EpochInfo;

use sui_types::sui_system_state::SuiSystemState;

use crate::EpochData;
use crate::tables::DEFAULT_COLUMN;

pub const NAME: &str = "epochs";

pub mod col {
    // Epoch Start columns
    pub const EPOCH: &str = "ep";
    pub const PROTOCOL_VERSION: &str = "pv";
    pub const START_TIMESTAMP: &str = "st";
    pub const START_CHECKPOINT: &str = "sc";
    pub const REFERENCE_GAS_PRICE: &str = "rg";
    pub const SYSTEM_STATE: &str = "ss";

    // Epoch End columns
    pub const END_TIMESTAMP: &str = "et";
    pub const END_CHECKPOINT: &str = "ec";
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

/// Encode epoch start data to individual columns.
/// Required fields enforced by function signature.
pub fn encode_start(
    epoch: u64,
    protocol_version: u64,
    start_timestamp_ms: u64,
    start_checkpoint: u64,
    reference_gas_price: u64,
    system_state: &SuiSystemState,
) -> Result<Vec<(&'static str, Bytes)>> {
    Ok(vec![
        (col::EPOCH, Bytes::from(epoch.to_be_bytes().to_vec())),
        (
            col::PROTOCOL_VERSION,
            Bytes::from(protocol_version.to_be_bytes().to_vec()),
        ),
        (
            col::START_TIMESTAMP,
            Bytes::from(start_timestamp_ms.to_be_bytes().to_vec()),
        ),
        (
            col::START_CHECKPOINT,
            Bytes::from(start_checkpoint.to_be_bytes().to_vec()),
        ),
        (
            col::REFERENCE_GAS_PRICE,
            Bytes::from(reference_gas_price.to_be_bytes().to_vec()),
        ),
        (col::SYSTEM_STATE, Bytes::from(bcs::to_bytes(system_state)?)),
    ])
}

/// Encode epoch end data to individual columns.
pub fn encode_end(end_timestamp_ms: u64, end_checkpoint: u64) -> [(&'static str, Bytes); 2] {
    [
        (
            col::END_TIMESTAMP,
            Bytes::from(end_timestamp_ms.to_be_bytes().to_vec()),
        ),
        (
            col::END_CHECKPOINT,
            Bytes::from(end_checkpoint.to_be_bytes().to_vec()),
        ),
    ]
}

pub fn decode(row: &[(Bytes, Bytes)]) -> Result<EpochData> {
    let mut data = EpochData::default();

    for (col, value) in row {
        if let b"" = col.as_ref() {
            let info: EpochInfo = bcs::from_bytes(value)?;
            data.epoch = Some(info.epoch);
            data.protocol_version = info.protocol_version;
            data.start_timestamp_ms = info.start_timestamp_ms;
            data.start_checkpoint = info.start_checkpoint;
            data.reference_gas_price = info.reference_gas_price;
            data.system_state = info.system_state;
            data.end_timestamp_ms = info.end_timestamp_ms;
            data.end_checkpoint = info.end_checkpoint;
        }
    }

    Ok(data)
}
