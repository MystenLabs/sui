// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

use std::{
    collections::{BTreeSet, HashSet},
    sync::Arc,
};

use sui_protocol_config::ProtocolConfig;
use sui_types::{
    base_types::{ObjectRef, SuiAddress, TxContext},
    committee::EpochId,
    digests::TransactionDigest,
    effects::TransactionEffects,
    error::{ExecutionError, SuiError},
    execution::{ExecutionState, TypeLayoutStore},
    execution_mode::{self, ExecutionResult},
    gas::SuiGasStatus,
    metrics::LimitsMetrics,
    temporary_store::{InnerTemporaryStore, TemporaryStore},
    transaction::{ProgrammableTransaction, TransactionKind},
    type_resolver::LayoutResolver,
};

use move_vm_runtime_latest::move_vm::MoveVM;
use sui_adapter_latest::adapter::new_move_vm;
use sui_adapter_latest::execution_engine::execute_transaction_to_effects;
use sui_adapter_latest::programmable_transactions;
use sui_adapter_latest::type_layout_resolver::TypeLayoutResolver;
use sui_move_natives_latest::all_natives;

use crate::executor::Executor;

pub(crate) struct VM(Arc<MoveVM>);

impl VM {
    pub(crate) fn new(
        protocol_config: &ProtocolConfig,
        paranoid_type_checks: bool,
        silent: bool,
    ) -> Result<Self, SuiError> {
        Ok(VM(Arc::new(new_move_vm(
            all_natives(silent),
            protocol_config,
            paranoid_type_checks,
        )?)))
    }
}

impl Executor for VM {
    fn execute_transaction_to_effects(
        &self,
        protocol_config: &ProtocolConfig,
        metrics: Arc<LimitsMetrics>,
        enable_expensive_checks: bool,
        certificate_deny_set: &HashSet<TransactionDigest>,
        epoch_id: &EpochId,
        epoch_timestamp_ms: u64,
        temporary_store: TemporaryStore,
        shared_object_refs: Vec<ObjectRef>,
        gas_status: SuiGasStatus,
        gas: &[ObjectRef],
        transaction_kind: TransactionKind,
        transaction_signer: SuiAddress,
        transaction_digest: TransactionDigest,
        transaction_dependencies: BTreeSet<TransactionDigest>,
    ) -> (
        InnerTemporaryStore,
        TransactionEffects,
        Result<(), ExecutionError>,
    ) {
        execute_transaction_to_effects::<execution_mode::Normal>(
            shared_object_refs,
            temporary_store,
            transaction_kind,
            transaction_signer,
            gas,
            transaction_digest,
            transaction_dependencies,
            &self.0,
            gas_status,
            epoch_id,
            epoch_timestamp_ms,
            protocol_config,
            metrics,
            enable_expensive_checks,
            certificate_deny_set,
        )
    }

    fn dev_inspect_transaction(
        &self,
        protocol_config: &ProtocolConfig,
        metrics: Arc<LimitsMetrics>,
        enable_expensive_checks: bool,
        certificate_deny_set: &HashSet<TransactionDigest>,
        epoch_id: &EpochId,
        epoch_timestamp_ms: u64,
        temporary_store: TemporaryStore,
        shared_object_refs: Vec<ObjectRef>,
        gas_status: SuiGasStatus,
        gas: &[ObjectRef],
        transaction_kind: TransactionKind,
        transaction_signer: SuiAddress,
        transaction_digest: TransactionDigest,
        transaction_dependencies: BTreeSet<TransactionDigest>,
    ) -> (
        InnerTemporaryStore,
        TransactionEffects,
        Result<Vec<ExecutionResult>, ExecutionError>,
    ) {
        execute_transaction_to_effects::<execution_mode::DevInspect>(
            shared_object_refs,
            temporary_store,
            transaction_kind,
            transaction_signer,
            gas,
            transaction_digest,
            transaction_dependencies,
            &self.0,
            gas_status,
            epoch_id,
            epoch_timestamp_ms,
            protocol_config,
            metrics,
            enable_expensive_checks,
            certificate_deny_set,
        )
    }

    fn update_genesis_state(
        &self,
        protocol_config: &ProtocolConfig,
        metrics: Arc<LimitsMetrics>,
        state_view: &mut dyn ExecutionState,
        tx_context: &mut TxContext,
        gas_status: &mut SuiGasStatus,
        pt: ProgrammableTransaction,
    ) -> Result<(), ExecutionError> {
        programmable_transactions::execution::execute::<execution_mode::Genesis>(
            protocol_config,
            metrics,
            &self.0,
            state_view,
            tx_context,
            gas_status,
            None,
            pt,
        )
    }

    fn type_layout_resolver<'r, 'vm: 'r, 'store: 'r>(
        &'vm self,
        store: Box<dyn TypeLayoutStore + 'store>,
    ) -> Box<dyn LayoutResolver + 'r> {
        Box::new(TypeLayoutResolver::new(&self.0, store))
    }
}
