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
    pub const CP_HI: &str = "ch";
    pub const TX_HI: &str = "th";
    pub const SAFE_MODE: &str = "sm";
    pub const TOTAL_STAKE: &str = "ts";
    pub const STORAGE_FUND_BALANCE: &str = "sf";
    pub const STORAGE_FUND_REINVESTMENT: &str = "sr";
    pub const STORAGE_CHARGE: &str = "sg";
    pub const STORAGE_REBATE: &str = "sb";
    pub const STAKE_SUBSIDY_AMOUNT: &str = "sa";
    pub const TOTAL_GAS_FEES: &str = "tg";
    pub const TOTAL_STAKE_REWARDS_DISTRIBUTED: &str = "td";
    pub const LEFTOVER_STORAGE_FUND_INFLOW: &str = "lf";
    pub const EPOCH_COMMITMENTS: &str = "cm";
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
pub fn encode_end(
    end_timestamp_ms: u64,
    end_checkpoint: u64,
    cp_hi: u64,
    tx_hi: u64,
    safe_mode: bool,
    total_stake: Option<u64>,
    storage_fund_balance: Option<u64>,
    storage_fund_reinvestment: Option<u64>,
    storage_charge: Option<u64>,
    storage_rebate: Option<u64>,
    stake_subsidy_amount: Option<u64>,
    total_gas_fees: Option<u64>,
    total_stake_rewards_distributed: Option<u64>,
    leftover_storage_fund_inflow: Option<u64>,
    epoch_commitments: &[u8],
) -> Vec<(&'static str, Bytes)> {
    let mut cols = vec![
        (
            col::END_TIMESTAMP,
            Bytes::from(end_timestamp_ms.to_be_bytes().to_vec()),
        ),
        (
            col::END_CHECKPOINT,
            Bytes::from(end_checkpoint.to_be_bytes().to_vec()),
        ),
        (col::CP_HI, Bytes::from(cp_hi.to_be_bytes().to_vec())),
        (col::TX_HI, Bytes::from(tx_hi.to_be_bytes().to_vec())),
        (col::SAFE_MODE, Bytes::from(vec![u8::from(safe_mode)])),
        (
            col::EPOCH_COMMITMENTS,
            Bytes::from(epoch_commitments.to_vec()),
        ),
    ];

    let optional_fields: [(&str, Option<u64>); 9] = [
        (col::TOTAL_STAKE, total_stake),
        (col::STORAGE_FUND_BALANCE, storage_fund_balance),
        (col::STORAGE_FUND_REINVESTMENT, storage_fund_reinvestment),
        (col::STORAGE_CHARGE, storage_charge),
        (col::STORAGE_REBATE, storage_rebate),
        (col::STAKE_SUBSIDY_AMOUNT, stake_subsidy_amount),
        (col::TOTAL_GAS_FEES, total_gas_fees),
        (
            col::TOTAL_STAKE_REWARDS_DISTRIBUTED,
            total_stake_rewards_distributed,
        ),
        (
            col::LEFTOVER_STORAGE_FUND_INFLOW,
            leftover_storage_fund_inflow,
        ),
    ];

    for (name, value) in optional_fields {
        if let Some(v) = value {
            cols.push((name, Bytes::from(v.to_be_bytes().to_vec())));
        }
    }

    cols
}

pub fn decode(row: &[(Bytes, Bytes)]) -> Result<EpochData> {
    let mut data = EpochData::default();

    for (col, value) in row {
        match col.as_ref() {
            // Legacy format: empty column contains BCS-serialized EpochInfo
            b"" => {
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
            // New format: individual columns
            b"ep" => data.epoch = Some(u64::from_be_bytes(value.as_ref().try_into()?)),
            b"pv" => data.protocol_version = Some(u64::from_be_bytes(value.as_ref().try_into()?)),
            b"st" => data.start_timestamp_ms = Some(u64::from_be_bytes(value.as_ref().try_into()?)),
            b"sc" => data.start_checkpoint = Some(u64::from_be_bytes(value.as_ref().try_into()?)),
            b"rg" => {
                data.reference_gas_price = Some(u64::from_be_bytes(value.as_ref().try_into()?))
            }
            b"ss" => data.system_state = Some(bcs::from_bytes(value)?),
            b"et" => data.end_timestamp_ms = Some(u64::from_be_bytes(value.as_ref().try_into()?)),
            b"ec" => data.end_checkpoint = Some(u64::from_be_bytes(value.as_ref().try_into()?)),
            b"ch" => data.cp_hi = Some(u64::from_be_bytes(value.as_ref().try_into()?)),
            b"th" => data.tx_hi = Some(u64::from_be_bytes(value.as_ref().try_into()?)),
            b"sm" => data.safe_mode = Some(value.as_ref().first().copied().unwrap_or(0) != 0),
            b"ts" => data.total_stake = Some(u64::from_be_bytes(value.as_ref().try_into()?)),
            b"sf" => {
                data.storage_fund_balance = Some(u64::from_be_bytes(value.as_ref().try_into()?))
            }
            b"sr" => {
                data.storage_fund_reinvestment =
                    Some(u64::from_be_bytes(value.as_ref().try_into()?))
            }
            b"sg" => data.storage_charge = Some(u64::from_be_bytes(value.as_ref().try_into()?)),
            b"sb" => data.storage_rebate = Some(u64::from_be_bytes(value.as_ref().try_into()?)),
            b"sa" => {
                data.stake_subsidy_amount = Some(u64::from_be_bytes(value.as_ref().try_into()?))
            }
            b"tg" => data.total_gas_fees = Some(u64::from_be_bytes(value.as_ref().try_into()?)),
            b"td" => {
                data.total_stake_rewards_distributed =
                    Some(u64::from_be_bytes(value.as_ref().try_into()?))
            }
            b"lf" => {
                data.leftover_storage_fund_inflow =
                    Some(u64::from_be_bytes(value.as_ref().try_into()?))
            }
            b"cm" => data.epoch_commitments = Some(value.to_vec()),
            _ => {}
        }
    }

    Ok(data)
}
