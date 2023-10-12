// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

//! This module contains the transactional test runner instantiation for the Sui adapter

pub mod args;
pub mod programmable_transaction_test_parser;
pub mod test_adapter;

use move_transactional_test_runner::framework::run_test_impl;
use std::path::Path;
use sui_types::storage::ObjectStore;
use test_adapter::{SuiTestAdapter, PRE_COMPILED};

use std::sync::Arc;
use sui_core::authority::authority_test_utils::send_and_confirm_transaction_with_execution_error;
use sui_core::authority::AuthorityState;
use sui_json_rpc_types::DevInspectResults;
use sui_json_rpc_types::EventFilter;
use sui_json_rpc_types::SuiEvent;
use sui_storage::key_value_store::TransactionKeyValueStore;
use sui_types::base_types::ObjectID;
use sui_types::base_types::SuiAddress;
use sui_types::base_types::VersionNumber;
use sui_types::effects::TransactionEffects;
use sui_types::error::ExecutionError;
use sui_types::error::SuiError;
use sui_types::error::SuiResult;
use sui_types::event::EventID;
use sui_types::messages_checkpoint::VerifiedCheckpoint;
use sui_types::object::Object;
use sui_types::transaction::Transaction;
use sui_types::transaction::TransactionDataAPI;
use sui_types::transaction::TransactionKind;

#[cfg_attr(not(msim), tokio::main)]
#[cfg_attr(msim, msim::main)]
pub async fn run_test(path: &Path) -> Result<(), Box<dyn std::error::Error>> {
    run_test_impl::<SuiTestAdapter>(path, Some(&*PRE_COMPILED)).await?;
    Ok(())
}

pub struct ValidatorWithFullnode {
    pub validator: Arc<AuthorityState>,
    pub fullnode: Arc<AuthorityState>,
    pub kv_store: Arc<TransactionKeyValueStore>,
}

#[allow(unused_variables)]
/// TODO: better name?
#[async_trait::async_trait]
pub trait TransactionalAdapter: Send + Sync + ObjectStore {
    async fn execute_txn(
        &mut self,
        transaction: Transaction,
    ) -> anyhow::Result<(TransactionEffects, Option<ExecutionError>)>;

    async fn create_checkpoint(&mut self) -> anyhow::Result<VerifiedCheckpoint>;

    async fn advance_clock(
        &mut self,
        duration: std::time::Duration,
    ) -> anyhow::Result<TransactionEffects>;

    async fn advance_epoch(&mut self) -> anyhow::Result<()>;

    async fn request_gas(
        &mut self,
        address: SuiAddress,
        amount: u64,
    ) -> anyhow::Result<TransactionEffects>;

    async fn dev_inspect_transaction_block(
        &self,
        sender: SuiAddress,
        transaction_kind: TransactionKind,
        gas_price: Option<u64>,
    ) -> SuiResult<DevInspectResults>;

    async fn query_events(
        &self,
        query: EventFilter,
        // If `Some`, the query will start from the next item after the specified cursor
        cursor: Option<EventID>,
        limit: usize,
        descending: bool,
    ) -> SuiResult<Vec<SuiEvent>>;
}

#[async_trait::async_trait]
impl TransactionalAdapter for ValidatorWithFullnode {
    async fn execute_txn(
        &mut self,
        transaction: Transaction,
    ) -> anyhow::Result<(TransactionEffects, Option<ExecutionError>)> {
        let with_shared = transaction
            .data()
            .intent_message()
            .value
            .contains_shared_object();
        let (_, effects, execution_error) = send_and_confirm_transaction_with_execution_error(
            &self.validator,
            Some(&self.fullnode),
            transaction,
            with_shared,
        )
        .await?;
        Ok((effects.into_data(), execution_error))
    }

    async fn dev_inspect_transaction_block(
        &self,
        sender: SuiAddress,
        transaction_kind: TransactionKind,
        gas_price: Option<u64>,
    ) -> SuiResult<DevInspectResults> {
        self.fullnode
            .dev_inspect_transaction_block(sender, transaction_kind, gas_price)
            .await
    }

    async fn query_events(
        &self,
        query: EventFilter,
        // If `Some`, the query will start from the next item after the specified cursor
        cursor: Option<EventID>,
        limit: usize,
        descending: bool,
    ) -> SuiResult<Vec<SuiEvent>> {
        self.validator
            .query_events(&self.kv_store, query, cursor, limit, descending)
            .await
    }

    async fn create_checkpoint(&mut self) -> anyhow::Result<VerifiedCheckpoint> {
        unimplemented!("create_checkpoint not supported")
    }

    async fn advance_clock(
        &mut self,
        _duration: std::time::Duration,
    ) -> anyhow::Result<TransactionEffects> {
        unimplemented!("advance_clock not supported")
    }

    async fn advance_epoch(&mut self) -> anyhow::Result<()> {
        unimplemented!("advance_epoch not supported")
    }

    async fn request_gas(
        &mut self,
        _address: SuiAddress,
        _amount: u64,
    ) -> anyhow::Result<TransactionEffects> {
        unimplemented!("request_gas not supported")
    }
}

impl ObjectStore for ValidatorWithFullnode {
    fn get_object(&self, object_id: &ObjectID) -> Result<Option<Object>, SuiError> {
        self.validator.database.get_object(object_id)
    }

    fn get_object_by_key(
        &self,
        object_id: &ObjectID,
        version: VersionNumber,
    ) -> Result<Option<Object>, SuiError> {
        self.validator
            .database
            .get_object_by_key(object_id, version)
    }
}
