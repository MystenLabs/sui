// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

use crate::static_programmable_transactions::{
    loading::ast as L, metering::translation_meter::TranslationMeter,
};
use sui_types::error::ExecutionError;

/// After loading and before type checking we do a pass over the loaded transaction to charge for
/// types that occured in the transaction and were loaded. We simply charge for the number of type
/// nodes that were loaded.
pub fn meter(
    meter: &mut TranslationMeter,
    transaction: &L::Transaction,
) -> Result<(), ExecutionError> {
    let inputs = transaction.inputs.iter().filter_map(|i| match &i.1 {
        L::InputType::Bytes => None,
        L::InputType::Fixed(ty) => Some(ty),
    });
    let commands = transaction.commands.iter().flat_map(command_types);
    for ty in inputs.chain(commands) {
        meter.charge_num_type_nodes(ty.node_count())?;
    }
    Ok(())
}

fn command_types(cmd: &L::Command) -> Box<dyn Iterator<Item = &L::Type> + '_> {
    match cmd {
        L::Command::MoveCall(move_call) => Box::new(
            move_call
                .function
                .type_arguments
                .iter()
                .chain(move_call.function.signature.parameters.iter())
                .chain(move_call.function.signature.return_.iter()),
        ),
        L::Command::MakeMoveVec(Some(ty), _) => Box::new(std::iter::once(ty)),
        L::Command::TransferObjects(_, _)
        | L::Command::SplitCoins(_, _)
        | L::Command::MergeCoins(_, _)
        | L::Command::MakeMoveVec(None, _)
        | L::Command::Publish(_, _, _)
        | L::Command::Upgrade(_, _, _, _, _) => Box::new(std::iter::empty()),
    }
}
