// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

use std::collections::{BTreeMap, BTreeSet};

use move_core_types::account_address::AccountAddress;
use sui_types::{
    SUI_ACCUMULATOR_ROOT_OBJECT_ID, SUI_FRAMEWORK_ADDRESS, SUI_FRAMEWORK_PACKAGE_ID,
    accumulator_root::{
        ACCUMULATOR_ROOT_SETTLE_U128_FUNC, ACCUMULATOR_ROOT_SETTLEMENT_PROLOGUE_FUNC,
        ACCUMULATOR_ROOT_SETTLEMENT_SETTLE_EVENTS_FUNC, ACCUMULATOR_SETTLEMENT_MODULE,
    },
    transaction::{
        Argument, CallArg, Command, ObjectArg, ProgrammableTransaction, SharedObjectMutability,
    },
};

pub(crate) struct ExecutionBatch {
    pub transaction: ProgrammableTransaction,
    pub command_indices: Vec<usize>,
}

/// Partition settlement execution by the defining addresses in each update's type. Combining
/// unrelated types can introduce a dependency cycle into their otherwise valid linkages.
/// These are execution batches only: the transaction digest and context remain the original ones.
pub(crate) fn execution_batches(transaction: ProgrammableTransaction) -> Vec<ExecutionBatch> {
    let groups = settlement_groups(&transaction);
    let Some(mut groups) = groups.filter(|groups| groups.len() > 1) else {
        return vec![ExecutionBatch {
            command_indices: (0..transaction.commands.len()).collect(),
            transaction,
        }];
    };

    // The prologue records conservation totals for the entire transaction, exactly once.
    groups.first_entry().unwrap().get_mut().insert(0, 0);
    let ProgrammableTransaction { inputs, commands } = transaction;
    let mut commands: Vec<_> = commands.into_iter().map(Some).collect();
    groups
        .into_values()
        .map(|command_indices| {
            let mut batch_inputs = vec![];
            let mut input_indices = BTreeMap::new();
            let batch_commands = command_indices
                .iter()
                .map(|&i| {
                    let Command::MoveCall(mut call) = commands[i].take().unwrap() else {
                        unreachable!("settlement commands were checked before partitioning")
                    };
                    for argument in &mut call.arguments {
                        let Argument::Input(i) = argument else {
                            unreachable!("settlement arguments were checked before partitioning")
                        };
                        *i = *input_indices.entry(*i).or_insert_with(|| {
                            let new_index = u16::try_from(batch_inputs.len())
                                .expect("a batch has no more inputs than the original transaction");
                            batch_inputs.push(inputs[usize::from(*i)].clone());
                            new_index
                        });
                    }
                    Command::MoveCall(call)
                })
                .collect();
            ExecutionBatch {
                transaction: ProgrammableTransaction {
                    inputs: batch_inputs,
                    commands: batch_commands,
                },
                command_indices,
            }
        })
        .collect()
}

fn settlement_groups(
    transaction: &ProgrammableTransaction,
) -> Option<BTreeMap<BTreeSet<AccountAddress>, Vec<usize>>> {
    let mut root = None;
    for (i, input) in transaction.inputs.iter().enumerate() {
        match input {
            CallArg::Pure(_) => (),
            CallArg::Object(ObjectArg::SharedObject {
                id,
                mutability: SharedObjectMutability::NonExclusiveWrite,
                ..
            }) if *id == SUI_ACCUMULATOR_ROOT_OBJECT_ID && root.is_none() => {
                root = Some(Argument::Input(u16::try_from(i).ok()?));
            }
            _ => return None,
        }
    }
    let root = root?;
    let mut groups: BTreeMap<BTreeSet<AccountAddress>, Vec<usize>> = BTreeMap::new();
    for (i, command) in transaction.commands.iter().enumerate() {
        let Command::MoveCall(call) = command else {
            return None;
        };
        if call.package != SUI_FRAMEWORK_PACKAGE_ID
            || call.module != ACCUMULATOR_SETTLEMENT_MODULE.as_str()
            || call.arguments.first() != Some(&root)
        {
            return None;
        }
        let (function, arity, arguments) = if i == 0 {
            (ACCUMULATOR_ROOT_SETTLEMENT_PROLOGUE_FUNC, 0, 6)
        } else if call.function == ACCUMULATOR_ROOT_SETTLE_U128_FUNC.as_str() {
            (ACCUMULATOR_ROOT_SETTLE_U128_FUNC, 1, 4)
        } else {
            (ACCUMULATOR_ROOT_SETTLEMENT_SETTLE_EVENTS_FUNC, 0, 5)
        };
        if call.function != function.as_str()
            || call.type_arguments.len() != arity
            || call.arguments.len() != arguments
            || !call.arguments.iter().skip(1).all(|argument| {
                matches!(argument, Argument::Input(i)
                    if matches!(transaction.inputs.get(usize::from(*i)), Some(CallArg::Pure(_))))
            })
        {
            return None;
        }
        if i != 0 {
            // settle_events has no type arguments; its EventStreamHead is a framework type.
            let mut addresses = BTreeSet::from([SUI_FRAMEWORK_ADDRESS]);
            for ty in &call.type_arguments {
                addresses.extend(ty.to_type_tag().ok()?.all_addresses());
            }
            groups.entry(addresses).or_default().push(i);
        }
    }
    Some(groups)
}
