// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

use diesel::{Insertable, Queryable};
use jsonrpsee::core::__reexports::serde::Deserialize;

use sui_json_rpc_types::{EndOfEpochInfo, EpochInfo};

use crate::errors::IndexerError;
use crate::models::system_state::DBValidatorSummary;
use crate::schema::epochs;

#[derive(Queryable, Insertable, Debug, Clone, Default)]
#[diesel(table_name = epochs)]
pub struct DBEpochInfo {
    pub epoch: i64,
    pub first_checkpoint_id: i64,
    pub last_checkpoint_id: Option<i64>,
    pub epoch_start_timestamp: i64,
    pub epoch_end_timestamp: Option<i64>,
    pub epoch_total_transactions: i64,
    pub next_epoch_version: Option<i64>,
    pub next_epoch_committee: Vec<Option<Vec<u8>>>,
    pub next_epoch_committee_stake: Vec<Option<i64>>,
    pub epoch_commitments: Vec<Option<Vec<u8>>>,

    /// existing fields from `SystemEpochInfo`
    pub protocol_version: Option<i64>,
    pub reference_gas_price: Option<i64>,
    pub total_stake: Option<i64>,
    pub storage_fund_reinvestment: Option<i64>,
    pub storage_charge: Option<i64>,
    pub storage_rebate: Option<i64>,
    pub storage_fund_balance: Option<i64>,
    pub stake_subsidy_amount: Option<i64>,
    pub total_gas_fees: Option<i64>,
    pub total_stake_rewards_distributed: Option<i64>,
    pub leftover_storage_fund_inflow: Option<i64>,
}

impl DBEpochInfo {
    pub fn to_epoch_info(
        self,
        validators: Vec<DBValidatorSummary>,
    ) -> Result<EpochInfo, IndexerError> {
        let validators = validators
            .into_iter()
            .map(|v| v.try_into())
            .collect::<Result<_, _>>()?;
        let end_of_epoch_info = (
            self.last_checkpoint_id,
            self.epoch_end_timestamp,
            self.protocol_version,
            self.reference_gas_price,
            self.total_stake,
            self.storage_fund_reinvestment,
            self.storage_charge,
            self.storage_rebate,
            self.storage_fund_balance,
            self.stake_subsidy_amount,
            self.total_gas_fees,
            self.total_stake_rewards_distributed,
            self.leftover_storage_fund_inflow,
        );

        let end_of_epoch_info = if let (
            Some(last_checkpoint_id),
            Some(epoch_end_timestamp),
            Some(protocol_version),
            Some(reference_gas_price),
            Some(total_stake),
            Some(storage_fund_reinvestment),
            Some(storage_charge),
            Some(storage_rebate),
            Some(storage_fund_balance),
            Some(stake_subsidy_amount),
            Some(total_gas_fees),
            Some(total_stake_rewards_distributed),
            Some(leftover_storage_fund_inflow),
        ) = end_of_epoch_info
        {
            Some(EndOfEpochInfo {
                last_checkpoint_id: last_checkpoint_id as u64,
                epoch_end_timestamp: epoch_end_timestamp as u64,
                protocol_version: protocol_version as u64,
                reference_gas_price: reference_gas_price as u64,
                total_stake: total_stake as u64,
                storage_fund_reinvestment: storage_fund_reinvestment as u64,
                storage_charge: storage_charge as u64,
                storage_rebate: storage_rebate as u64,
                storage_fund_balance: storage_fund_balance as u64,
                stake_subsidy_amount: stake_subsidy_amount as u64,
                total_gas_fees: total_gas_fees as u64,
                total_stake_rewards_distributed: total_stake_rewards_distributed as u64,
                leftover_storage_fund_inflow: leftover_storage_fund_inflow as u64,
            })
        } else {
            None
        };

        Ok(EpochInfo {
            epoch: self.epoch as u64,
            validators,
            epoch_total_transactions: self.epoch_total_transactions as u64,
            first_checkpoint_id: self.first_checkpoint_id as u64,
            epoch_start_timestamp: self.epoch_start_timestamp as u64,
            end_of_epoch_info,
            reference_gas_price: self.reference_gas_price.map(|v| v as u64),
        })
    }
}

#[derive(Deserialize)]
pub struct SystemEpochInfoEvent {
    pub epoch: i64,
    pub protocol_version: i64,
    pub reference_gas_price: i64,
    pub total_stake: i64,
    pub storage_fund_reinvestment: i64,
    pub storage_charge: i64,
    pub storage_rebate: i64,
    pub storage_fund_balance: i64,
    pub stake_subsidy_amount: i64,
    pub total_gas_fees: i64,
    pub total_stake_rewards_distributed: i64,
    pub leftover_storage_fund_inflow: i64,
}
