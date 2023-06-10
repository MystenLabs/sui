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
    error::ExecutionError,
    execution::{ExecutionState, TypeLayoutStore},
    execution_mode::ExecutionResult,
    gas::GasCharger,
    metrics::LimitsMetrics,
    temporary_store::{InnerTemporaryStore, TemporaryStore},
    transaction::{ProgrammableTransaction, TransactionKind},
    type_resolver::LayoutResolver,
};

/// Abstracts over access to the VM across versions of the execution layer.
pub trait Executor {
    fn execute_transaction_to_effects(
        &self,
        // Configuration
        protocol_config: &ProtocolConfig,
        metrics: Arc<LimitsMetrics>,
        enable_expensive_checks: bool,
        certificate_deny_set: &HashSet<TransactionDigest>,
        // Epoch
        epoch_id: &EpochId,
        epoch_timestamp_ms: u64,
        // Transaction Inputs
        temporary_store: TemporaryStore,
        shared_object_refs: Vec<ObjectRef>,
        gas_charger: &mut GasCharger,
        // Transaction
        transaction_kind: TransactionKind,
        transaction_signer: SuiAddress,
        transaction_digest: TransactionDigest,
        transaction_dependencies: BTreeSet<TransactionDigest>,
    ) -> (
        InnerTemporaryStore,
        TransactionEffects,
        Result<(), ExecutionError>,
    );

    fn dev_inspect_transaction(
        &self,
        // Configuration
        protocol_config: &ProtocolConfig,
        metrics: Arc<LimitsMetrics>,
        enable_expensive_checks: bool,
        certificate_deny_set: &HashSet<TransactionDigest>,
        // Epoch
        epoch_id: &EpochId,
        epoch_timestamp_ms: u64,
        // Transaction Inputs
        temporary_store: TemporaryStore,
        shared_object_refs: Vec<ObjectRef>,
        gas_charger: &mut GasCharger,
        // Transaction
        transaction_kind: TransactionKind,
        transaction_signer: SuiAddress,
        transaction_digest: TransactionDigest,
        transaction_dependencies: BTreeSet<TransactionDigest>,
    ) -> (
        InnerTemporaryStore,
        TransactionEffects,
        Result<Vec<ExecutionResult>, ExecutionError>,
    );

    fn update_genesis_state(
        &self,
        // Configuration
        protocol_config: &ProtocolConfig,
        metrics: Arc<LimitsMetrics>,
        // Genesis State
        state_view: &mut dyn ExecutionState,
        tx_context: &mut TxContext,
        gas_charger: &mut GasCharger,
        // Transaction
        pt: ProgrammableTransaction,
    ) -> Result<(), ExecutionError>;

    fn type_layout_resolver<'r, 'vm: 'r, 'store: 'r>(
        &'vm self,
        store: Box<dyn TypeLayoutStore + 'store>,
    ) -> Box<dyn LayoutResolver + 'r>;
}
