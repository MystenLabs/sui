// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

use move_trace_format::format::MoveTraceBuilder;
use std::collections::BTreeMap;
use std::sync::Arc;
use sui_protocol_config::ProtocolConfig;
use sui_types::execution::ExecutionTiming;
use sui_types::execution_params::ExecutionOrEarlyError;
use sui_types::storage::BackingStore;
use sui_types::transaction::GasData;
use sui_types::{
    base_types::{ObjectID, SequenceNumber, SuiAddress},
    committee::EpochId,
    digests::TransactionDigest,
    effects::TransactionEffects,
    error::ExecutionError,
    execution::{ExecutionResult, ExecutionRetryError, TypeLayoutStore},
    execution_status::ExecutionFailure,
    gas::SuiGasStatus,
    inner_temporary_store::InnerTemporaryStore,
    layout_resolver::LayoutResolver,
    metrics::ExecutionMetrics,
    transaction::{CheckedInputObjects, ProgrammableTransaction, TransactionKind},
};

/// Output of executing a transaction to effects: temporary store, gas status, effects, per-command
/// timings, and the execution status (`E` is the error detail type).
pub type TransactionEffectsOutput<E> = (
    InnerTemporaryStore,
    SuiGasStatus,
    TransactionEffects,
    Vec<ExecutionTiming>,
    Result<(), E>,
);

/// Abstracts over access to the VM across versions of the execution layer.
pub trait Executor {
    fn execute_transaction_to_effects(
        &self,
        store: &dyn BackingStore,
        // Configuration
        protocol_config: &ProtocolConfig,
        metrics: Arc<ExecutionMetrics>,
        enable_expensive_checks: bool,
        execution_params: ExecutionOrEarlyError,
        // Epoch
        epoch_id: &EpochId,
        epoch_timestamp_ms: u64,
        // Transaction Inputs
        input_objects: CheckedInputObjects,
        // Versions of system objects this transaction may read, keyed by object ID.
        system_object_versions: BTreeMap<ObjectID, SequenceNumber>,
        // Gas related
        gas: GasData,
        gas_status: SuiGasStatus,
        // Transaction
        transaction_kind: TransactionKind,
        rewritten_inputs: Option<Vec<bool>>,
        transaction_signer: SuiAddress,
        transaction_digest: TransactionDigest,
        trace_builder_opt: &mut Option<MoveTraceBuilder>,
    ) -> Result<TransactionEffectsOutput<ExecutionFailure>, ExecutionRetryError>;

    /// Execution mode returns greater error information, primarily used in fullnode execution
    /// as opposed to `execute_transaction_to_effects` which only includes basic `ExecutionFailure` error.
    fn execute_transaction_to_effects_and_execution_error(
        &self,
        store: &dyn BackingStore,
        protocol_config: &ProtocolConfig,
        metrics: Arc<ExecutionMetrics>,
        enable_expensive_checks: bool,
        execution_params: ExecutionOrEarlyError,
        epoch_id: &EpochId,
        epoch_timestamp_ms: u64,
        input_objects: CheckedInputObjects,
        system_object_versions: BTreeMap<ObjectID, SequenceNumber>,
        gas: GasData,
        gas_status: SuiGasStatus,
        transaction_kind: TransactionKind,
        _rewritten_inputs: Option<Vec<bool>>,
        transaction_signer: SuiAddress,
        transaction_digest: TransactionDigest,
        trace_builder_opt: &mut Option<MoveTraceBuilder>,
    ) -> Result<TransactionEffectsOutput<ExecutionError>, ExecutionRetryError>;

    fn dev_inspect_transaction(
        &self,
        store: &dyn BackingStore,
        // Configuration
        protocol_config: &ProtocolConfig,
        metrics: Arc<ExecutionMetrics>,
        enable_expensive_checks: bool,
        execution_params: ExecutionOrEarlyError,
        // Epoch
        epoch_id: &EpochId,
        epoch_timestamp_ms: u64,
        // Transaction Inputs
        input_objects: CheckedInputObjects,
        // Gas related
        gas: GasData,
        gas_status: SuiGasStatus,
        // Transaction
        transaction_kind: TransactionKind,
        rewritten_inputs: Option<Vec<bool>>,
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
        metrics: Arc<ExecutionMetrics>,
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
        protocol_config: &'vm ProtocolConfig,
        store: Box<dyn TypeLayoutStore + 'store>,
    ) -> Box<dyn LayoutResolver + 'r>;
}
