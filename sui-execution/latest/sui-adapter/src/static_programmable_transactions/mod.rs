// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

#![deny(clippy::arithmetic_side_effects)]

use crate::{
    data_store::cached_package_store::CachedPackageStore,
    execution_mode::ExecutionMode,
    execution_value::ExecutionState,
    gas_charger::GasCharger,
    static_programmable_transactions::{
        env::Env, linkage::analysis::LinkageAnalyzer, metering::translation_meter,
    },
    temporary_store::TemporaryStore,
};
use linkage::resolved_linkage::RootedLinkage;
use move_trace_format::format::MoveTraceBuilder;
use move_vm_runtime::move_vm::MoveVM;
use std::{cell::RefCell, rc::Rc, sync::Arc};
use sui_protocol_config::ProtocolConfig;
use sui_types::{
    base_types::{TxContext, TxContextKind},
    error::ExecutionError,
    execution::ResultWithTimings,
    metrics::LimitsMetrics,
    storage::{BackingPackageStore, BackingStore},
    transaction::{
        Argument, CallArg, Command, ObjectArg, ProgrammableMoveCall, ProgrammableTransaction,
    },
};

// TODO we might replace this with a new one
pub use crate::data_store::legacy::linkage_view::LinkageView;

pub mod env;
pub mod execution;
pub mod linkage;
pub mod loading;
pub mod metering;
pub mod spanned;
pub mod typing;

/// Attempt to find all owned object inputs that are still live at the end of the
/// PTB. Only works for inputs that do not have drop or copy. Returns a vector of
/// booleans, one for each input, indicating whether the input is still live.
/// For any input which is not an owned object input, the corresponding bool is
/// undefined.
pub fn find_live_inputs<Mode: ExecutionMode>(
    protocol_config: &ProtocolConfig,
    vm: &MoveVM,
    backing_store: &dyn BackingStore,
    txn: &ProgrammableTransaction,
    epoch_id: u64,
) -> Option<Vec<bool>> {
    let package_store = CachedPackageStore::new(Box::new(backing_store));
    let linkage_analysis = LinkageAnalyzer::new::<Mode>(protocol_config).expect("cannot fail");

    // make an empty temporary store. never consulted during function loading.
    let mut temporary_store = TemporaryStore::new(
        backing_store,
        Default::default(),
        Default::default(),
        Default::default(),
        protocol_config,
        epoch_id,
    );

    let env = Env::new(
        protocol_config,
        vm,
        &mut temporary_store,
        &package_store,
        &linkage_analysis,
    );

    // All inputs start as live. We mark them as dead when the appear as an input
    // to a non-reference argument of a command.
    let mut live_inputs = vec![true; txn.inputs.len()];

    for command in txn.commands.iter() {
        match command {
            Command::MoveCall(pmc) => {
                let resolved_linkage = env
                    .linkage_analysis
                    .compute_call_linkage(pmc, env.linkable_store)
                    .ok()?;
                let ProgrammableMoveCall {
                    package,
                    module,
                    function: name,
                    type_arguments: ptype_arguments,
                    arguments,
                } = &**pmc;
                let linkage = RootedLinkage::new(**package, resolved_linkage);
                let type_arguments = ptype_arguments
                    .iter()
                    .enumerate()
                    .map(|(idx, ty)| env.load_type_input(idx, ty.clone()))
                    .collect::<Result<Vec<_>, _>>()
                    .ok()?;
                let function = env
                    .load_function(
                        *package,
                        module.to_string(),
                        name.to_string(),
                        type_arguments,
                        linkage,
                    )
                    .ok()?;

                let signature = &function.signature;

                let num_params = if !matches!(match signature.parameters.last() {
                    Some(ty) => ty.is_tx_context(),
                    None => TxContextKind::None,
                }, TxContextKind::None) {
                    signature.parameters.len().checked_sub(1).unwrap()
                } else {
                    signature.parameters.len()
                };

                if num_params != arguments.len() {
                    // error will be ignored and caught by execution, no need for source
                    return None;
                }

                for (arg, type_) in arguments.iter().zip(signature.parameters.iter().take(num_params)) {
                    let Argument::Input(input_idx) = arg else {
                        continue;
                    };
                    let input_idx = *input_idx as usize;

                    let input = txn.inputs.get(input_idx)?;

                    let CallArg::Object(ObjectArg::ImmOrOwnedObject(_)) = input else {
                        continue;
                    };

                    // objects are no longer live after they are taken by value
                    if !type_.is_reference() {
                        live_inputs[input_idx] = false;
                    }
                }
            }
            // TransferObjects, MergeCoins, and MakeMoveVec take by value
            Command::TransferObjects(arguments, _) |
            Command::MergeCoins(_, arguments) |
            Command::MakeMoveVec(_, arguments) => {
                for argument in arguments.iter() {
                    let Argument::Input(input_idx) = argument else {
                        continue;
                    };
                    let input_idx = *input_idx as usize;
                    live_inputs[input_idx] = false;
                }
            }
            // SplitCoins only takes `&mut Coin<T>`
            Command::SplitCoins(..) |
            // Publish and Upgrade don't take object inputs
            Command::Publish(..) |
            Command::Upgrade(..) => ()
        }
    }

    Some(live_inputs)
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
    let linkage_analysis =
        LinkageAnalyzer::new::<Mode>(protocol_config).map_err(|e| (e, vec![]))?;

    let mut env = Env::new(
        protocol_config,
        vm,
        state_view,
        &package_store,
        &linkage_analysis,
    );
    let mut translation_meter =
        translation_meter::TranslationMeter::new(protocol_config, gas_charger);

    let txn = {
        let tx_context_ref = tx_context.borrow();
        loading::translate::transaction(&mut translation_meter, &env, &tx_context_ref, txn)
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
