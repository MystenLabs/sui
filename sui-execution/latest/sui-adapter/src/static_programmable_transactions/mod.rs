// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

#![deny(clippy::arithmetic_side_effects)]
#![deny(clippy::indexing_slicing)]
#![deny(clippy::cast_possible_truncation)]

use crate::{
    data_store::{
        cached_package_store::CachedPackageStore,
        transaction_package_store::TransactionPackageStore,
    },
    execution_mode::ExecutionMode,
    execution_value::ExecutionState,
    metering::gas_charger::GasMeter,
    static_programmable_transactions::{
        env::Env, linkage::config::ResolutionConfig, metering::translation_meter,
    },
};
use move_trace_format::format::MoveTraceBuilder;
use move_vm_runtime::runtime::MoveRuntime;
use std::{cell::RefCell, rc::Rc, sync::Arc};
use sui_protocol_config::ProtocolConfig;
use sui_types::{
    base_types::TxContext, error::ExecutionErrorTrait, execution::ResultWithTimings,
    execution_status::ExecutionErrorKind, metrics::ExecutionMetrics, storage::BackingPackageStore,
    transaction::ProgrammableTransaction,
};

pub mod env;
pub mod execution;
pub mod linkage;
pub mod linking;
pub mod loading;
pub mod metering;
pub mod spanned;
pub mod typing;

pub(crate) fn execute<Mode: ExecutionMode>(
    protocol_config: &ProtocolConfig,
    metrics: Arc<ExecutionMetrics>,
    vm: &MoveRuntime,
    state_view: &mut dyn ExecutionState,
    package_store: &dyn BackingPackageStore,
    tx_context: Rc<RefCell<TxContext>>,
    gas_meter: &mut GasMeter,
    // which inputs are withdrawals that need to be converted to coins
    withdrawal_compatibility_inputs: Option<Vec<bool>>,
    txn: ProgrammableTransaction,
    trace_builder_opt: &mut Option<MoveTraceBuilder>,
) -> ResultWithTimings<Mode::ExecutionResults, Mode::Error> {
    let gas_payment = gas_meter.payment();
    let package_store = CachedPackageStore::new(vm, TransactionPackageStore::new(package_store));
    let resolution_config = ResolutionConfig::from_protocol_config::<Mode>(protocol_config);
    let mut translation_meter =
        translation_meter::TranslationMeter::new(protocol_config, gas_meter);
    let txn = linking::translate::transaction::<Mode>(
        &mut translation_meter,
        &resolution_config,
        &package_store,
        state_view,
        protocol_config,
        txn,
    )
    .map_err(|e| (e, vec![]))?;
    let unified_linkage = txn.unified_linkage.clone();
    let loading_vm = vm
        .make_vm(
            &package_store.package_store,
            unified_linkage
                .linkage_context::<Mode::Error>()
                .map_err(|e| (e, vec![]))?,
        )
        .map_err(|e| {
            (
                Mode::Error::new_with_source(ExecutionErrorKind::InvalidLinkage, e),
                vec![],
            )
        })?;

    let mut env: Env<Mode> = Env::new(
        protocol_config,
        vm,
        state_view,
        &package_store,
        &resolution_config,
        &loading_vm,
        &unified_linkage,
    );

    let txn = {
        let tx_context_ref = tx_context.borrow();
        loading::translate::transaction::<Mode>(
            &mut translation_meter,
            &env,
            &tx_context_ref,
            withdrawal_compatibility_inputs,
            gas_payment,
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
        gas_meter,
        txn,
        trace_builder_opt,
    )
}
