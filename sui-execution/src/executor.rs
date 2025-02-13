// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

use move_trace_format::format::MoveTraceBuilder;
use std::{collections::HashSet, sync::Arc};
use sui_protocol_config::ProtocolConfig;
use sui_types::execution::ExecutionTiming;
use sui_types::storage::BackingStore;
use sui_types::{
    base_types::{ObjectRef, SuiAddress},
    committee::EpochId,
    digests::TransactionDigest,
    effects::TransactionEffects,
    error::ExecutionError,
    execution::{ExecutionResult, TypeLayoutStore},
    gas::SuiGasStatus,
    inner_temporary_store::InnerTemporaryStore,
    layout_resolver::LayoutResolver,
    metrics::LimitsMetrics,
    transaction::{CheckedInputObjects, ProgrammableTransaction, TransactionKind},
};

/// Abstracts over access to the VM across versions of the execution layer.
pub trait Executor {
    fn execute_transaction_to_effects(
        &self,
        store: &dyn BackingStore,
        // Configuration
        protocol_config: &ProtocolConfig,
        metrics: Arc<LimitsMetrics>,
        enable_expensive_checks: bool,
        certificate_deny_set: &HashSet<TransactionDigest>,
        // Epoch
        epoch_id: &EpochId,
        epoch_timestamp_ms: u64,
        // Transaction Inputs
        input_objects: CheckedInputObjects,
        // Gas related
        gas_coins: Vec<ObjectRef>,
        gas_status: SuiGasStatus,
        // Transaction
        transaction_kind: TransactionKind,
        transaction_signer: SuiAddress,
        transaction_digest: TransactionDigest,
        trace_builder_opt: &mut Option<MoveTraceBuilder>,
    ) -> (
        InnerTemporaryStore,
        SuiGasStatus,
        TransactionEffects,
        Vec<ExecutionTiming>,
        Result<(), ExecutionError>,
    );

    fn dev_inspect_transaction(
        &self,
        store: &dyn BackingStore,
        // Configuration
        protocol_config: &ProtocolConfig,
        metrics: Arc<LimitsMetrics>,
        enable_expensive_checks: bool,
        certificate_deny_set: &HashSet<TransactionDigest>,
        // Epoch
        epoch_id: &EpochId,
        epoch_timestamp_ms: u64,
        // Transaction Inputs
        input_objects: CheckedInputObjects,
        // Gas related
        gas_coins: Vec<ObjectRef>,
        gas_status: SuiGasStatus,
        // Transaction
        transaction_kind: TransactionKind,
        transaction_signer: SuiAddress,
        transaction_digest: TransactionDigest,
        skip_all_checks: bool,
    ) -> (
        InnerTemporaryStore,
        SuiGasStatus,
        TransactionEffects,
        Result<Vec<ExecutionResult>, ExecutionError>,
    );

    fn update_genesis_state(
        &self,
        store: &dyn BackingStore,
        // Configuration
        protocol_config: &ProtocolConfig,
        metrics: Arc<LimitsMetrics>,
        // Epoch
        epoch_id: EpochId,
        epoch_timestamp_ms: u64,
        // Genesis Digest
        transaction_digest: &TransactionDigest,
        // Transaction
        input_objects: CheckedInputObjects,
        pt: ProgrammableTransaction,
    ) -> Result<InnerTemporaryStore, ExecutionError>;

    fn type_layout_resolver<'r, 'vm: 'r, 'store: 'r>(
        &'vm self,
        store: Box<dyn TypeLayoutStore + 'store>,
    ) -> Box<dyn LayoutResolver + 'r>;
}
