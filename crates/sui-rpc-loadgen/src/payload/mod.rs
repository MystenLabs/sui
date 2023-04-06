// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

mod checkpoint_utils;
mod get_all_balances;
mod get_balance;
mod get_checkpoints;
mod get_object;
mod get_reference_gas_price;
mod multi_get_objects;
mod multi_get_transaction_blocks;
mod pay_sui;
mod query_transactions;
mod rpc_command_processor;
mod validation;
use strum_macros::EnumString;

use anyhow::Result;
use async_trait::async_trait;
use core::default::Default;
use std::time::Duration;
use sui_types::{
    base_types::SuiAddress, digests::TransactionDigest,
    messages_checkpoint::CheckpointSequenceNumber,
};

use crate::load_test::LoadTestConfig;
pub use rpc_command_processor::{
    load_addresses_from_file, load_addresses_with_coin_types_from_file, load_digests_from_file,
    load_objects_from_file, RpcCommandProcessor,
};
use sui_types::base_types::ObjectID;

use self::rpc_command_processor::AddressWithCoinTypes;

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
        address_type: AddressQueryType,
        addresses: Vec<SuiAddress>,
    ) -> Self {
        let query_transactions = QueryTransactionBlocks {
            address_type,
            addresses,
        };
        Self {
            data: CommandData::QueryTransactionBlocks(query_transactions),
            ..Default::default()
        }
    }

    pub fn new_multi_get_transaction_blocks(digests: Vec<TransactionDigest>) -> Self {
        let multi_get_transaction_blocks = MultiGetTransactionBlocks { digests };
        Self {
            data: CommandData::MultiGetTransactionBlocks(multi_get_transaction_blocks),
            ..Default::default()
        }
    }

    pub fn new_multi_get_objects(object_ids: Vec<ObjectID>) -> Self {
        let multi_get_objects = MultiGetObjects { object_ids };
        Self {
            data: CommandData::MultiGetObjects(multi_get_objects),
            ..Default::default()
        }
    }

    pub fn new_get_object(object_ids: Vec<ObjectID>, chunk_size: usize) -> Self {
        let get_object = GetObject {
            object_ids,
            chunk_size,
        };
        Self {
            data: CommandData::GetObject(get_object),
            ..Default::default()
        }
    }

    pub fn new_get_all_balances(
        addresses: Vec<SuiAddress>,
        chunk_size: usize,
        record: bool,
    ) -> Self {
        let get_all_balances = GetAllBalances {
            addresses,
            chunk_size,
            record,
        };
        Self {
            data: CommandData::GetAllBalances(get_all_balances),
            ..Default::default()
        }
    }

    pub fn new_get_balance(
        addresses_with_coin_types: Vec<AddressWithCoinTypes>,
        chunk_size: usize,
    ) -> Self {
        // load data here
        let get_balance = GetBalance {
            addresses_with_coin_types,
            chunk_size,
        };
        Self {
            data: CommandData::GetBalance(get_balance),
            ..Default::default()
        }
    }

    pub fn new_get_reference_gas_price(num_repeats: usize) -> Self {
        let get_reference_gas_price = GetReferenceGasPrice { num_repeats };
        Self {
            data: CommandData::GetReferenceGasPrice(get_reference_gas_price),
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
    QueryTransactionBlocks(QueryTransactionBlocks),
    MultiGetTransactionBlocks(MultiGetTransactionBlocks),
    MultiGetObjects(MultiGetObjects),
    GetObject(GetObject),
    GetAllBalances(GetAllBalances),
    GetBalance(GetBalance),
    GetReferenceGasPrice(GetReferenceGasPrice),
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
pub struct QueryTransactionBlocks {
    pub address_type: AddressQueryType,
    pub addresses: Vec<SuiAddress>,
}

#[derive(Clone)]
pub struct MultiGetTransactionBlocks {
    pub digests: Vec<TransactionDigest>,
}

#[derive(Clone, EnumString)]
#[strum(serialize_all = "lowercase")]
pub enum AddressQueryType {
    From,
    To,
    Both,
}

impl Default for AddressQueryType {
    fn default() -> Self {
        AddressQueryType::From
    }
}

#[derive(Clone)]
pub struct MultiGetObjects {
    pub object_ids: Vec<ObjectID>,
}

#[derive(Clone)]
pub struct GetObject {
    pub object_ids: Vec<ObjectID>,
    pub chunk_size: usize,
}

#[derive(Clone)]
pub struct GetAllBalances {
    pub addresses: Vec<SuiAddress>,
    pub chunk_size: usize,
    pub record: bool,
}

#[derive(Clone)]
pub struct GetBalance {
    pub addresses_with_coin_types: Vec<AddressWithCoinTypes>,
    pub chunk_size: usize,
}

#[derive(Clone)]
pub struct GetReferenceGasPrice {
    num_repeats: usize,
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
