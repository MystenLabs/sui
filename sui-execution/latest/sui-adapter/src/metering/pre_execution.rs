// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

use sui_protocol_config::ProtocolConfig;
use sui_types::transaction::{Command, ProgrammableTransaction, TransactionKind};

// MIST owed before executing a PTB.
pub(crate) fn charge(transaction_kind: &TransactionKind, protocol_config: &ProtocolConfig) -> u64 {
    let TransactionKind::ProgrammableTransaction(pt) = transaction_kind else {
        return 0;
    };

    publish_upgrade_charge(pt, protocol_config)
        .saturating_add(mutable_system_object_input_charge(pt, protocol_config))
}

fn publish_upgrade_charge(pt: &ProgrammableTransaction, protocol_config: &ProtocolConfig) -> u64 {
    if !protocol_config.package_publish_charge() {
        return 0;
    }

    let fixed = protocol_config.package_publish_charge_fixed();
    let per_module = protocol_config.package_publish_charge_per_module();
    let per_dependency = protocol_config.package_publish_charge_per_dependency();
    pt.commands
        .iter()
        .map(|command| match command {
            Command::Publish(modules, dependencies)
            | Command::Upgrade(modules, dependencies, _, _) => fixed
                .saturating_add((modules.len() as u64).saturating_mul(per_module))
                .saturating_add((dependencies.len() as u64).saturating_mul(per_dependency)),
            Command::MoveCall(_)
            | Command::TransferObjects(_, _)
            | Command::SplitCoins(_, _)
            | Command::MergeCoins(_, _)
            | Command::MakeMoveVec(_, _) => 0,
        })
        .fold(0u64, |total, charge| total.saturating_add(charge))
}

fn mutable_system_object_input_charge(
    pt: &ProgrammableTransaction,
    protocol_config: &ProtocolConfig,
) -> u64 {
    let (Some(ids), Some(charge)) = (
        protocol_config.mutable_system_object_input_charge_ids(),
        protocol_config.mutable_system_object_input_charge_as_option(),
    ) else {
        return 0;
    };

    pt.shared_input_objects()
        .filter(|input| input.may_mutate() && ids.contains(&input.id.into()))
        .fold(0u64, |total, _| total.saturating_add(charge))
}
