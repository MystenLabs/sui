// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

use crate::metering::pre_execution::charge;
use sui_protocol_config::ProtocolConfig;
use sui_types::{
    SUI_CLOCK_OBJECT_ID, SUI_FREEZE_LIST_OBJECT_ID, SUI_SYSTEM_STATE_OBJECT_SHARED_VERSION,
    base_types::ObjectID,
    transaction::{
        CallArg, Command, ObjectArg, ProgrammableTransaction, SharedObjectMutability,
        TransactionKind,
    },
};

fn transaction(inputs: Vec<CallArg>, commands: Vec<Command>) -> TransactionKind {
    TransactionKind::ProgrammableTransaction(ProgrammableTransaction { inputs, commands })
}

fn shared_input(id: ObjectID, mutability: SharedObjectMutability) -> CallArg {
    CallArg::Object(ObjectArg::SharedObject {
        id,
        initial_shared_version: SUI_SYSTEM_STATE_OBJECT_SHARED_VERSION,
        mutability,
    })
}

#[test]
fn charges_configured_mutable_system_object_inputs() {
    let config = ProtocolConfig::get_for_max_version_UNSAFE();
    let tx = transaction(
        vec![
            shared_input(SUI_FREEZE_LIST_OBJECT_ID, SharedObjectMutability::Mutable),
            shared_input(SUI_CLOCK_OBJECT_ID, SharedObjectMutability::Immutable),
        ],
        vec![],
    );

    assert_eq!(
        charge(&tx, &config),
        config.mutable_system_object_input_charge()
    );
}

#[test]
fn adds_mutable_system_object_input_charge_to_publish_charge() {
    let config = ProtocolConfig::get_for_max_version_UNSAFE();
    let tx = transaction(
        vec![shared_input(
            SUI_FREEZE_LIST_OBJECT_ID,
            SharedObjectMutability::Mutable,
        )],
        vec![Command::Publish(vec![vec![]], vec![])],
    );

    assert_eq!(
        charge(&tx, &config),
        config
            .mutable_system_object_input_charge()
            .saturating_add(config.package_publish_charge_fixed())
            .saturating_add(config.package_publish_charge_per_module()),
    );
}

#[test]
fn does_not_charge_when_mutable_system_object_input_charge_is_disabled() {
    let mut config = ProtocolConfig::get_for_max_version_UNSAFE();
    config.disable_mutable_system_object_input_charge_for_testing();
    let tx = transaction(
        vec![shared_input(
            SUI_FREEZE_LIST_OBJECT_ID,
            SharedObjectMutability::Mutable,
        )],
        vec![],
    );

    assert_eq!(charge(&tx, &config), 0);
}
