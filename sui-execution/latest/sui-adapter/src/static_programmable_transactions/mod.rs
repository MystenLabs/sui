// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

use crate::{execution_value::ExecutionState, gas_charger::GasCharger};
use move_trace_format::format::MoveTraceBuilder;
use move_vm_runtime::move_vm::MoveVM;
use std::{cell::RefCell, rc::Rc, sync::Arc};
use sui_protocol_config::ProtocolConfig;
use sui_types::{
    base_types::TxContext, error::ExecutionError, execution::ResultWithTimings,
    metrics::LimitsMetrics, transaction::ProgrammableTransaction,
};

// TODO we might replace this with a new one
pub use crate::programmable_transactions::linkage_view::LinkageView;

pub mod env;
pub mod execution;
pub mod loading;
pub mod spanned;
pub mod typing;

pub fn execute<'pc, 'vm, 'state, 'linkage>(
    protocol_config: &'pc ProtocolConfig,
    vm: &'vm MoveVM,
    state_view: &'state mut dyn ExecutionState,
    linkage_view: &'linkage LinkageView<'state>,
    metrics: Arc<LimitsMetrics>,
    tx_context: Rc<RefCell<TxContext>>,
    gas_charger: &mut GasCharger,
    txn: ProgrammableTransaction,
    trace_builder_opt: &mut Option<MoveTraceBuilder>,
) -> ResultWithTimings<(), ExecutionError> {
    use crate::static_programmable_transactions::env::Env;

    let mut env = Env::new(protocol_config, vm, state_view, linkage_view);
    let txn = loading::translate::transaction(&env, txn).map_err(|e| (e, vec![]))?;
    let txn = typing::translate_and_verify(&env, txn).map_err(|e| (e, vec![]))?;
    execution::interpreter::execute(
        &mut env,
        metrics,
        tx_context,
        gas_charger,
        txn,
        trace_builder_opt,
    )
}
