// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

use sui_protocol_config::ProtocolConfig;
use sui_types::transaction::{Command, TransactionKind};

/// MIST owed for the Publish and Upgrade commands of a PTB.
pub(crate) fn publish_upgrade_charge(
    transaction_kind: &TransactionKind,
    protocol_config: &ProtocolConfig,
) -> u64 {
    if !protocol_config.package_publish_charge() {
        return 0;
    }
    let TransactionKind::ProgrammableTransaction(pt) = transaction_kind else {
        return 0;
    };
    pt.commands
        .iter()
        .map(|command| match command {
            Command::Publish(modules, dependencies)
            | Command::Upgrade(modules, dependencies, _, _) => protocol_config
                .package_publish_charge_fixed()
                .saturating_add(
                    (modules.len() as u64)
                        .saturating_mul(protocol_config.package_publish_charge_per_module()),
                )
                .saturating_add(
                    (dependencies.len() as u64)
                        .saturating_mul(protocol_config.package_publish_charge_per_dependency()),
                ),
            _ => 0,
        })
        .fold(0u64, |total, charge| total.saturating_add(charge))
}
