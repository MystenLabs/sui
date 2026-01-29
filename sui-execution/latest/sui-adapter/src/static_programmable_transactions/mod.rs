// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

#![deny(clippy::arithmetic_side_effects)]

use crate::{
    data_store::{
        cached_package_store::CachedPackageStore,
        transaction_package_store::TransactionPackageStore,
    },
    execution_mode::ExecutionMode,
    execution_value::ExecutionState,
    gas_charger::GasCharger,
    static_programmable_transactions::{
        env::Env, linkage::analysis::LinkageAnalyzer, metering::translation_meter,
    },
};
use move_trace_format::format::MoveTraceBuilder;
use move_vm_runtime::runtime::MoveRuntime;
use std::{cell::RefCell, rc::Rc, sync::Arc};
use sui_protocol_config::ProtocolConfig;
use sui_types::{
    base_types::TxContext,
    error::{ExecutionError, ExecutionErrorKind},
    execution::ResultWithTimings,
    metrics::LimitsMetrics,
    storage::BackingPackageStore,
    transaction::ProgrammableTransaction,
};

pub mod env;
pub mod execution;
pub mod linkage;
pub mod loading;
pub mod metering;
pub mod spanned;
pub mod typing;

pub fn execute<Mode: ExecutionMode>(
    protocol_config: &ProtocolConfig,
    metrics: Arc<LimitsMetrics>,
    vm: &MoveRuntime,
    state_view: &mut dyn ExecutionState,
    package_store: &dyn BackingPackageStore,
    tx_context: Rc<RefCell<TxContext>>,
    gas_charger: &mut GasCharger,
    // which inputs are withdrawals that need to be converted to coins
    withdrawal_compatibility_inputs: Option<Vec<bool>>,
    txn: ProgrammableTransaction,
    trace_builder_opt: &mut Option<MoveTraceBuilder>,
) -> ResultWithTimings<Mode::ExecutionResults, ExecutionError> {
    let package_store = CachedPackageStore::new(vm, TransactionPackageStore::new(package_store));
    let linkage_analysis =
        LinkageAnalyzer::new::<Mode>(protocol_config).map_err(|e| (e, vec![]))?;
    let ptb_type_linkage = linkage_analysis
        .compute_input_type_resolution_linkage(&txn, &package_store, state_view)
        .map_err(|e| (e, vec![]))?;
    let resolution_vm = vm
        .make_vm(
            &package_store.package_store,
            ptb_type_linkage.linkage_context(),
        )
        .map_err(|e| {
            (
                ExecutionError::new_with_source(ExecutionErrorKind::InvalidLinkage, e),
                vec![],
            )
        })?;

    let mut env = Env::new(
        protocol_config,
        vm,
        state_view,
        &package_store,
        &linkage_analysis,
        &resolution_vm,
    );
    let mut translation_meter =
        translation_meter::TranslationMeter::new(protocol_config, gas_charger);

    let txn = {
        let tx_context_ref = tx_context.borrow();
        loading::translate::transaction::<Mode>(
            &mut translation_meter,
            &env,
            &tx_context_ref,
            withdrawal_compatibility_inputs,
            txn,
        )
        .map_err(|e| (e, vec![]))?
    };
    let txn = typing::translate_and_verify::<Mode>(&mut translation_meter, &env, txn)
        .map_err(|e| (e, vec![]))?;
    execution::interpreter::execute::<Mode>(
        &mut env,
        metrics,
        tx_context,
        gas_charger,
        txn,
        trace_builder_opt,
    )
}
