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
    execution::{ExecutionState, TypeLayoutStore},
    execution_mode::{self, ExecutionResult},
    gas::GasCharger,
    metrics::{BytecodeVerifierMetrics, LimitsMetrics},
    temporary_store::{InnerTemporaryStore, TemporaryStore},
    transaction::{ProgrammableTransaction, TransactionKind},
    type_resolver::LayoutResolver,
};

use move_vm_runtime_v0::move_vm::MoveVM;
use sui_adapter_v0::adapter::{
    default_verifier_config, new_move_vm, run_metered_move_bytecode_verifier,
};
use sui_adapter_v0::execution_engine::execute_transaction_to_effects;
use sui_adapter_v0::programmable_transactions;
use sui_adapter_v0::type_layout_resolver::TypeLayoutResolver;
use sui_move_natives_v0::all_natives;
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
        gas_charger: &mut GasCharger,
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
            gas_charger,
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
        protocol_config: &ProtocolConfig,
        metrics: Arc<LimitsMetrics>,
        enable_expensive_checks: bool,
        certificate_deny_set: &HashSet<TransactionDigest>,
        epoch_id: &EpochId,
        epoch_timestamp_ms: u64,
        temporary_store: TemporaryStore,
        shared_object_refs: Vec<ObjectRef>,
        gas_charger: &mut GasCharger,
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
            gas_charger,
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
        protocol_config: &ProtocolConfig,
        metrics: Arc<LimitsMetrics>,
        state_view: &mut dyn ExecutionState,
        tx_context: &mut TxContext,
        gas_charger: &mut GasCharger,
        pt: ProgrammableTransaction,
    ) -> Result<(), ExecutionError> {
        programmable_transactions::execution::execute::<execution_mode::Genesis>(
            protocol_config,
            metrics,
            &self.0,
            state_view,
            tx_context,
            gas_charger,
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
