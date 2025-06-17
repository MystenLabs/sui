// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

use crate::{
    data_store::cached_package_store::CachedPackageStore, execution_mode::ExecutionMode,
    execution_value::ExecutionState, gas_charger::GasCharger,
    static_programmable_transactions::env::Env,
};
use move_trace_format::format::MoveTraceBuilder;
use move_vm_runtime::move_vm::MoveVM;
use std::{cell::RefCell, rc::Rc, sync::Arc};
use sui_protocol_config::ProtocolConfig;
use sui_types::{
    base_types::TxContext, error::ExecutionError, execution::ResultWithTimings,
    metrics::LimitsMetrics, storage::BackingPackageStore, transaction::ProgrammableTransaction,
};

// TODO we might replace this with a new one
pub use crate::data_store::legacy::linkage_view::LinkageView;

pub mod env;
pub mod execution;
pub mod linkage;
pub mod loading;
pub mod spanned;
pub mod typing;

macro_rules! better_todo {
    ($($arg:tt)+) => {
        $crate::static_programmable_transactions::better_todo_(format!("{}", std::format_args!($($arg)+)))
    };
}
pub(crate) use better_todo;

pub fn better_todo_<T>(s: String) -> T {
    todo!("{}", s)
}

pub fn execute<Mode: ExecutionMode>(
    protocol_config: &ProtocolConfig,
    metrics: Arc<LimitsMetrics>,
    vm: &MoveVM,
    state_view: &mut dyn ExecutionState,
    package_store: &dyn BackingPackageStore,
    tx_context: Rc<RefCell<TxContext>>,
    gas_charger: &mut GasCharger,
    txn: ProgrammableTransaction,
    trace_builder_opt: &mut Option<MoveTraceBuilder>,
) -> ResultWithTimings<Mode::ExecutionResults, ExecutionError> {
    let package_store = CachedPackageStore::new(Box::new(package_store));
    let linkage_analysis = linkage::analysis::linkage_analysis_for_protocol_config::<Mode>(
        protocol_config,
        &txn,
        &package_store,
    )
    .map_err(|e| (e, vec![]))?;

    let mut env = Env::new(
        protocol_config,
        vm,
        state_view,
        &package_store,
        linkage_analysis.as_ref(),
    );
    let txn = loading::translate::transaction(&env, txn).map_err(|e| (e, vec![]))?;
    let txn = typing::translate_and_verify::<Mode>(&env, txn).map_err(|e| (e, vec![]))?;
    execution::interpreter::execute::<Mode>(
        &mut env,
        metrics,
        tx_context,
        gas_charger,
        txn,
        trace_builder_opt,
    )
}
