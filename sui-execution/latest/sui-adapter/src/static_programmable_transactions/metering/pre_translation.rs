// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

use crate::static_programmable_transactions::metering::translation_meter::TranslationMeter;
use sui_protocol_config::ProtocolConfig;
use sui_types::{
    error::ExecutionErrorTrait,
    transaction::{CallArg, Command, ProgrammableTransaction},
};

/// Before loading and type checking, we do a first pass over the transaction to charge for basic
/// properties:
/// - number of inputs and pure input bytes
/// - number of commands and their arguments
/// - for Move calls, count arguments to the function (both value and type arguments) as the
///   "arguments" to charge for for the command.
pub fn meter<E: ExecutionErrorTrait>(
    meter: &mut TranslationMeter,
    transaction: &ProgrammableTransaction,
) -> Result<(), E> {
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

/// Calculate the MIST charge for raw Publish and Upgrade commands before linkage analysis.
pub fn publish_upgrade_charge(
    transaction: &ProgrammableTransaction,
    config: &ProtocolConfig,
) -> u64 {
    transaction
        .commands
        .iter()
        .filter_map(|command| match command {
            Command::Publish(modules, dependencies)
            | Command::Upgrade(modules, dependencies, _, _) => {
                Some((modules.len(), dependencies.len()))
            }
            _ => None,
        })
        .fold(0, |total, (modules, dependencies)| {
            let charge = config
                .package_publish_charge_fixed()
                .saturating_add(
                    (modules as u64).saturating_mul(config.package_publish_charge_per_module()),
                )
                .saturating_add(
                    (dependencies as u64)
                        .saturating_mul(config.package_publish_charge_per_dependency()),
                );
            total.saturating_add(charge)
        })
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
