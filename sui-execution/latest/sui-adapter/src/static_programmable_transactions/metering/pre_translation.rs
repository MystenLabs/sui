// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

use crate::static_programmable_transactions::metering::translation_meter::TranslationMeter;
use sui_types::{
    error::ExecutionError,
    transaction::{CallArg, Command, ProgrammableTransaction},
};

/// Before loading and type checking, we do a first pass over the transaction to charge for basic
/// properties:
/// - number of inputs and pure input bytes
/// - number of commands and their arguments
/// - for Move calls, count arguments to the function (both value and type arguments) as the
///   "arguments" to charge for for the command.
pub fn meter(
    meter: &mut TranslationMeter,
    transaction: &ProgrammableTransaction,
) -> Result<(), ExecutionError> {
    meter.charge_base_inputs(transaction.inputs.len())?;

    for input in &transaction.inputs {
        match input {
            CallArg::Pure(bytes) => {
                meter.charge_pure_input_bytes(bytes.len())?;
            }
            CallArg::FundsWithdrawal(_) | CallArg::Object(_) => (),
        }
    }

    for command in &transaction.commands {
        meter.charge_base_command(arguments_len(command))?;
    }

    Ok(())
}

fn arguments_len(cmd: &Command) -> usize {
    match cmd {
        Command::MoveCall(call) => call
            .type_arguments
            .len()
            .saturating_add(call.arguments.len()),
        Command::TransferObjects(args, _)
        | Command::SplitCoins(_, args)
        | Command::MergeCoins(_, args) => args.len().saturating_add(1),
        Command::Publish(modules, deps) => modules.len().saturating_add(deps.len()),
        Command::MakeMoveVec(_, args) => args.len().saturating_add(1),
        Command::Upgrade(modules, deps, _, _) => {
            modules.len().saturating_add(deps.len()).saturating_add(2)
        }
    }
}
