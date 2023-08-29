// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

use std::{
    collections::{BTreeSet, HashSet},
    sync::Arc,
};

use move_binary_format::CompiledModule;
use move_vm_config::verifier::VerifierConfig;
use sui_protocol_config::ProtocolConfig;
use sui_types::{
    base_types::{ObjectRef, SuiAddress, TxContext},
    committee::EpochId,
    digests::TransactionDigest,
    effects::TransactionEffects,
    error::{ExecutionError, SuiError, SuiResult},
    execution::TypeLayoutStore,
    execution_mode::{self, ExecutionResult},
    gas::SuiGasStatus,
    inner_temporary_store::InnerTemporaryStore,
    metrics::{BytecodeVerifierMetrics, LimitsMetrics},
    transaction::{InputObjects, ProgrammableTransaction, TransactionKind},
    type_resolver::LayoutResolver,
};

use move_vm_runtime_v0::move_vm::MoveVM;
use sui_adapter_v0::adapter::{
    default_verifier_config, new_move_vm, run_metered_move_bytecode_verifier,
};
use sui_adapter_v0::execution_engine::execute_transaction_to_effects;
use sui_adapter_v0::gas_charger::GasCharger;
use sui_adapter_v0::programmable_transactions;
use sui_adapter_v0::temporary_store::TemporaryStore;
use sui_adapter_v0::type_layout_resolver::TypeLayoutResolver;
use sui_move_natives_v0::all_natives;
use sui_types::storage::BackingStore;
use sui_verifier_v0::meter::SuiVerifierMeter;

use crate::executor;
use crate::verifier;

pub(crate) struct Executor(Arc<MoveVM>);

pub(crate) struct Verifier<'m> {
    config: VerifierConfig,
    metrics: &'m Arc<BytecodeVerifierMetrics>,
    meter: SuiVerifierMeter,
}

impl Executor {
    pub(crate) fn new(
        protocol_config: &ProtocolConfig,
        paranoid_type_checks: bool,
        silent: bool,
    ) -> Result<Self, SuiError> {
        Ok(Executor(Arc::new(new_move_vm(
            all_natives(silent),
            protocol_config,
            paranoid_type_checks,
        )?)))
    }
}

impl<'m> Verifier<'m> {
    pub(crate) fn new(
        protocol_config: &ProtocolConfig,
        is_metered: bool,
        metrics: &'m Arc<BytecodeVerifierMetrics>,
    ) -> Self {
        let config = default_verifier_config(protocol_config, is_metered);
        let meter = SuiVerifierMeter::new(&config);
        Verifier {
            config,
            metrics,
            meter,
        }
    }
}

impl executor::Executor for Executor {
    fn execute_transaction_to_effects<'backing>(
        &self,
        store: Arc<dyn BackingStore + Send + Sync + 'backing>,
        protocol_config: &ProtocolConfig,
        metrics: Arc<LimitsMetrics>,
        enable_expensive_checks: bool,
        certificate_deny_set: &HashSet<TransactionDigest>,
        epoch_id: &EpochId,
        epoch_timestamp_ms: u64,
        input_objects: InputObjects,
        shared_object_refs: Vec<ObjectRef>,
        gas_coins: Vec<ObjectRef>,
        gas_status: SuiGasStatus,
        transaction_kind: TransactionKind,
        transaction_signer: SuiAddress,
        transaction_digest: TransactionDigest,
        transaction_dependencies: BTreeSet<TransactionDigest>,
    ) -> (
        InnerTemporaryStore,
        TransactionEffects,
        Result<(), ExecutionError>,
    ) {
        let temporary_store =
            TemporaryStore::new(store, input_objects, transaction_digest, protocol_config);
        let mut gas_charger =
            GasCharger::new(transaction_digest, gas_coins, gas_status, protocol_config);
        execute_transaction_to_effects::<execution_mode::Normal>(
            shared_object_refs,
            temporary_store,
            transaction_kind,
            transaction_signer,
            &mut gas_charger,
            transaction_digest,
            transaction_dependencies,
            &self.0,
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
        store: Arc<dyn BackingStore + Send + Sync>,
        protocol_config: &ProtocolConfig,
        metrics: Arc<LimitsMetrics>,
        enable_expensive_checks: bool,
        certificate_deny_set: &HashSet<TransactionDigest>,
        epoch_id: &EpochId,
        epoch_timestamp_ms: u64,
        input_objects: InputObjects,
        shared_object_refs: Vec<ObjectRef>,
        gas_coins: Vec<ObjectRef>,
        gas_status: SuiGasStatus,
        transaction_kind: TransactionKind,
        transaction_signer: SuiAddress,
        transaction_digest: TransactionDigest,
        transaction_dependencies: BTreeSet<TransactionDigest>,
    ) -> (
        InnerTemporaryStore,
        TransactionEffects,
        Result<Vec<ExecutionResult>, ExecutionError>,
    ) {
        let temporary_store = TemporaryStore::new_for_mock_transaction(
            store,
            input_objects,
            transaction_digest,
            protocol_config,
        );
        let mut gas_charger =
            GasCharger::new(transaction_digest, gas_coins, gas_status, protocol_config);
        execute_transaction_to_effects::<execution_mode::DevInspect>(
            shared_object_refs,
            temporary_store,
            transaction_kind,
            transaction_signer,
            &mut gas_charger,
            transaction_digest,
            transaction_dependencies,
            &self.0,
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
        store: Arc<dyn BackingStore + Send + Sync>,
        protocol_config: &ProtocolConfig,
        metrics: Arc<LimitsMetrics>,
        tx_context: &mut TxContext,
        input_objects: InputObjects,
        pt: ProgrammableTransaction,
    ) -> Result<InnerTemporaryStore, ExecutionError> {
        let mut temporary_store =
            TemporaryStore::new(store, input_objects, tx_context.digest(), protocol_config);
        let mut gas_charger = GasCharger::new_unmetered(tx_context.digest());
        programmable_transactions::execution::execute::<execution_mode::Genesis>(
            protocol_config,
            metrics,
            &self.0,
            &mut temporary_store,
            tx_context,
            &mut gas_charger,
            pt,
        )?;
        Ok(temporary_store.into_inner())
    }

    fn type_layout_resolver<'r, 'vm: 'r, 'store: 'r>(
        &'vm self,
        store: Box<dyn TypeLayoutStore + 'store>,
    ) -> Box<dyn LayoutResolver + 'r> {
        Box::new(TypeLayoutResolver::new(&self.0, store))
    }
}

impl<'m> verifier::Verifier for Verifier<'m> {
    fn meter_compiled_modules(
        &mut self,
        protocol_config: &ProtocolConfig,
        modules: &[CompiledModule],
    ) -> SuiResult<()> {
        run_metered_move_bytecode_verifier(
            modules,
            protocol_config,
            &self.config,
            &mut self.meter,
            self.metrics,
        )
    }
}
