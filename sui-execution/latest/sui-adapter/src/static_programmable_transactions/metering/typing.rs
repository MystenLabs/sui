// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

use crate::static_programmable_transactions::{
    metering::translation_meter::TranslationMeter, typing::ast as T,
};
use sui_types::error::ExecutionError;

/// After loading and type checking, we do a second pass over the typed transaction to charge for
/// type-related properties (before further analysis is done):
/// - number of type nodes (including nested)
/// - number of type references. These are charged non-linearly
pub fn meter(
    meter: &mut TranslationMeter,
    transaction: &T::Transaction,
) -> Result<(), ExecutionError> {
    let mut num_refs: u64 = 0;
    let mut num_nodes: u64 = 0;

    for ty in types(transaction) {
        if ty.is_reference() {
            num_refs = num_refs.saturating_add(1);
        }
        num_nodes = num_nodes.saturating_add(ty.node_count());
    }

    meter.charge_num_type_nodes(num_nodes)?;
    meter.charge_num_type_references(num_refs)?;
    Ok(())
}

fn types(txn: &T::Transaction) -> impl Iterator<Item = &T::Type> {
    let pure_types = txn.pure.iter().map(|p| &p.ty);
    let object_types = txn.objects.iter().map(|o| &o.ty);
    let receiving_types = txn.receiving.iter().map(|r| &r.ty);
    let command_types = txn.commands.iter().flat_map(command_types);
    pure_types
        .chain(object_types)
        .chain(receiving_types)
        .chain(command_types)
}

fn command_types(cmd: &T::Command) -> impl Iterator<Item = &T::Type> {
    let result_types = cmd.value.result_type.iter();
    let command_types = command_types_inner(&cmd.value.command);
    result_types.chain(command_types)
}

fn command_types_inner(cmd: &T::Command__) -> Box<dyn Iterator<Item = &T::Type> + '_> {
    match cmd {
        T::Command__::TransferObjects(args, arg) => {
            Box::new(std::iter::once(arg).chain(args.iter()).map(argument_type))
        }
        T::Command__::SplitCoins(ty, arg, args) | T::Command__::MergeCoins(ty, arg, args) => {
            Box::new(
                std::iter::once(arg)
                    .chain(args.iter())
                    .map(argument_type)
                    .chain(std::iter::once(ty)),
            )
        }
        T::Command__::MakeMoveVec(ty, args) => {
            Box::new(args.iter().map(argument_type).chain(std::iter::once(ty)))
        }
        T::Command__::MoveCall(call) => Box::new(
            call.arguments
                .iter()
                .map(argument_type)
                .chain(call.function.type_arguments.iter())
                .chain(call.function.signature.parameters.iter())
                .chain(call.function.signature.return_.iter()),
        ),
        T::Command__::Upgrade(_, _, _, arg, _) => Box::new(std::iter::once(arg).map(argument_type)),
        T::Command__::Publish(_, _, _) => Box::new(std::iter::empty()),
    }
}

fn argument_type(arg: &T::Argument) -> &T::Type {
    &arg.value.1
}
