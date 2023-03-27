// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

mod get_checkpoints;
mod pay_sui;
mod query_transactions;
mod rpc_command_processor;
mod validation;

use anyhow::Result;
use async_trait::async_trait;
use core::default::Default;
use std::str::FromStr;
use std::time::Duration;

use sui_types::messages_checkpoint::CheckpointSequenceNumber;

use crate::load_test::LoadTestConfig;
pub use rpc_command_processor::RpcCommandProcessor;
use sui_types::base_types::{ObjectID, SuiAddress};

#[derive(Default, Clone)]
pub struct SignerInfo {
    pub encoded_keypair: String,
    /// Different thread should use different gas_payment to avoid equivocation
    pub gas_payment: Option<Vec<ObjectID>>,
    pub gas_budget: Option<u64>,
}

impl SignerInfo {
    pub fn new(encoded_keypair: String) -> Self {
        Self {
            encoded_keypair,
            gas_payment: None,
            gas_budget: None,
        }
    }
}

#[derive(Clone, Default)]
pub struct Payload {
    pub commands: Vec<Command>,
    pub signer_info: Option<SignerInfo>,
}

#[derive(Default, Clone)]
pub struct Command {
    pub data: CommandData,
    /// 0 means the command will be run once. Default to be 0
    pub repeat_n_times: usize,
    /// how long to wait between the start of two subsequent repeats
    /// If the previous command takes longer than `repeat_interval` to finish, the next command
    /// will run as soon as the previous command finishes
    /// Default to be 0
    pub repeat_interval: Duration,
}

impl Command {
    pub fn new_dry_run() -> Self {
        Self {
            data: CommandData::DryRun(DryRun {}),
            ..Default::default()
        }
    }

    pub fn new_pay_sui() -> Self {
        Self {
            data: CommandData::PaySui(PaySui {}),
            ..Default::default()
        }
    }

    pub fn new_get_checkpoints(
        start: CheckpointSequenceNumber,
        end: Option<CheckpointSequenceNumber>,
        verify_transactions: bool,
        verify_objects: bool,
        record: bool,
    ) -> Self {
        Self {
            data: CommandData::GetCheckpoints(GetCheckpoints {
                start,
                end,
                verify_transactions,
                verify_objects,
                record,
            }),
            ..Default::default()
        }
    }

    pub fn new_query_transaction_blocks(
        address: Option<String>,
        address_type: Option<String>,
        from_file: Option<bool>,
    ) -> Self {
        let address_type = address_type.map(|s| match s.as_str() {
            "from" => AddressType::FromAddress,
            "to" => AddressType::ToAddress,
            _ => panic!("Invalid address type: {}", s),
        });

        let query_transactions = QueryTransactions {
            address: address.map(|addr| SuiAddress::from_str(&addr).unwrap()),
            address_type,
            from_file,
        };
        Self {
            data: CommandData::QueryTransactions(query_transactions),
            ..Default::default()
        }
    }

    pub fn with_repeat_n_times(mut self, num: usize) -> Self {
        self.repeat_n_times = num;
        self
    }

    pub fn with_repeat_interval(mut self, duration: Duration) -> Self {
        self.repeat_interval = duration;
        self
    }
}

#[derive(Clone)]
#[allow(dead_code)]
pub enum CommandData {
    DryRun(DryRun),
    GetCheckpoints(GetCheckpoints),
    PaySui(PaySui),
    QueryTransactions(QueryTransactions),
}

impl Default for CommandData {
    fn default() -> Self {
        CommandData::DryRun(DryRun {})
    }
}

#[derive(Clone)]
pub struct DryRun {}

#[derive(Clone, Default)]
pub struct GetCheckpoints {
    /// Default to start from 0
    pub start: CheckpointSequenceNumber,
    /// If None, use `getLatestCheckpointSequenceNumber`
    pub end: Option<CheckpointSequenceNumber>,
    pub verify_transactions: bool,
    pub verify_objects: bool,
    pub record: bool,
}

#[derive(Clone)]
pub struct PaySui {}

#[derive(Clone, Default)]
pub struct QueryTransactions {
    pub address: Option<SuiAddress>,
    pub address_type: Option<AddressType>,
    pub from_file: Option<bool>,
}

#[derive(Clone)]
pub enum AddressType {
    FromAddress,
    ToAddress,
}

#[async_trait]
pub trait Processor {
    /// process commands in order
    async fn apply(&self, payload: &Payload) -> Result<()>;

    /// prepare payload for each thread according to LoadTestConfig
    async fn prepare(&self, config: &LoadTestConfig) -> Result<Vec<Payload>>;

    /// write results to file based on LoadTestConfig
    fn dump_cache_to_file(&self, config: &LoadTestConfig);
}

/// all payload should implement this trait
#[async_trait]
pub trait ProcessPayload<'a, T> {
    async fn process(&'a self, op: T, signer_info: &Option<SignerInfo>) -> Result<()>;
}
