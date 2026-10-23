// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

use std::{collections::BTreeMap, sync::Arc};

use async_trait::async_trait;
use move_core_types::language_storage::StructTag;
use sui_types::{
    SUI_FRAMEWORK_PACKAGE_ID, SUI_PACKAGE_CONFIG_OBJECT_ID,
    base_types::{ObjectID, ObjectInfo, SequenceNumber, SuiAddress},
    digests::TransactionDigest,
    move_package::{MinVersionState, UpgradeCap, UpgradeCapPolicy, UpgradePolicy},
    object::{MoveObject, Object, Owner},
    transaction::{
        Argument, CallArg, Command, ObjectArg, SharedObjectMutability, TransactionData,
        TransactionDataAPI, TransactionKind,
    },
};

use super::{DataReader, TransactionBuilder};

struct TestDataReader {
    objects: BTreeMap<ObjectID, Object>,
}

#[async_trait]
impl DataReader for TestDataReader {
    async fn get_owned_objects(
        &self,
        _address: SuiAddress,
        _object_type: StructTag,
    ) -> anyhow::Result<Vec<ObjectInfo>> {
        unreachable!("tests provide an explicit gas object")
    }

    async fn get_object(&self, object_id: ObjectID) -> anyhow::Result<Object> {
        self.objects
            .get(&object_id)
            .cloned()
            .ok_or_else(|| anyhow::anyhow!("unexpected object request: {object_id}"))
    }

    async fn get_reference_gas_price(&self) -> anyhow::Result<u64> {
        Ok(1)
    }
}

fn upgrade_cap_object(policy: u8, owner: SuiAddress) -> Object {
    let cap_id = ObjectID::random();
    let mut cap = UpgradeCap::new(cap_id, ObjectID::random());
    cap.policy = policy;
    let move_object = unsafe {
        MoveObject::new_from_execution_with_limit(
            UpgradeCap::type_().into(),
            true,
            SequenceNumber::from_u64(1),
            bcs::to_bytes(&cap).unwrap(),
            u64::MAX,
        )
    }
    .unwrap();
    Object::new_move(
        move_object,
        Owner::AddressOwner(owner),
        TransactionDigest::genesis_marker(),
    )
}

fn test_builder(cap_policy: u8) -> (TransactionBuilder, Object, Object) {
    let owner = SuiAddress::random_for_testing_only();
    let cap = upgrade_cap_object(cap_policy, owner);
    let gas = Object::with_id_owner_for_testing(ObjectID::random(), owner);
    let package_config = Object::with_id_owner_version_for_testing(
        SUI_PACKAGE_CONFIG_OBJECT_ID,
        SequenceNumber::from_u64(7),
        Owner::Shared {
            initial_shared_version: SequenceNumber::from_u64(1),
        },
    );
    let objects = [cap.clone(), gas.clone(), package_config.clone()]
        .into_iter()
        .map(|object| (object.id(), object))
        .collect();
    (
        TransactionBuilder::new(Arc::new(TestDataReader { objects })),
        cap,
        gas,
    )
}

fn programmable(kind: &TransactionKind) -> &sui_types::transaction::ProgrammableTransaction {
    let TransactionKind::ProgrammableTransaction(pt) = kind else {
        panic!("expected a programmable transaction")
    };
    pt
}

fn move_call_names(pt: &sui_types::transaction::ProgrammableTransaction) -> Vec<(&str, &str)> {
    pt.commands
        .iter()
        .filter_map(|command| {
            let Command::MoveCall(call) = command else {
                return None;
            };
            Some((call.module.as_str(), call.function.as_str()))
        })
        .collect()
}

fn authorize_policy(pt: &sui_types::transaction::ProgrammableTransaction) -> u8 {
    let Command::MoveCall(call) = &pt.commands[0] else {
        panic!("expected authorize_upgrade")
    };
    assert_eq!(call.module.as_str(), "package");
    assert_eq!(call.function.as_str(), "authorize_upgrade");
    let Argument::Input(policy_input) = call.arguments[1] else {
        panic!("expected a pure policy input")
    };
    let CallArg::Pure(bytes) = &pt.inputs[policy_input as usize] else {
        panic!("expected a pure policy input")
    };
    bcs::from_bytes(bytes).unwrap()
}

fn has_mutable_package_config(pt: &sui_types::transaction::ProgrammableTransaction) -> bool {
    pt.inputs.iter().any(|input| {
        matches!(
            input,
            CallArg::Object(ObjectArg::SharedObject {
                id: SUI_PACKAGE_CONFIG_OBJECT_ID,
                mutability: SharedObjectMutability::Mutable,
                ..
            })
        )
    })
}

#[tokio::test]
async fn upgrade_tx_kind_uses_the_completion_flow_for_the_cap_state() {
    let (builder, cap, _) = test_builder(UpgradePolicy::Compatible as u8);
    let cases = [
        (
            UpgradePolicy::Compatible as u8 | MinVersionState::Available as u8,
            UpgradePolicy::Compatible as u8,
            false,
        ),
        (
            UpgradePolicy::Compatible as u8 | MinVersionState::PermanentlyDisabled as u8,
            UpgradePolicy::Compatible as u8,
            false,
        ),
        (
            UpgradePolicy::Compatible as u8 | MinVersionState::Enabled as u8,
            UpgradePolicy::Compatible as u8,
            true,
        ),
        (
            UpgradePolicy::Additive as u8 | MinVersionState::Enabled as u8,
            UpgradePolicy::Additive as u8,
            true,
        ),
        (
            UpgradePolicy::DepOnly as u8 | MinVersionState::Enabled as u8,
            UpgradePolicy::DepOnly as u8,
            true,
        ),
    ];

    for (packed_policy, base_policy, enrolled) in cases {
        let kind = builder
            .upgrade_tx_kind(
                ObjectID::random(),
                vec![vec![1, 2, 3]],
                vec![],
                cap.id(),
                UpgradeCapPolicy::try_from(packed_policy).unwrap(),
                vec![4, 5, 6],
            )
            .await
            .unwrap();
        let pt = programmable(&kind);

        assert_eq!(authorize_policy(pt), base_policy);
        assert_eq!(has_mutable_package_config(pt), enrolled);
        let expected_calls = if enrolled {
            vec![
                ("package", "authorize_upgrade"),
                ("package_config", "record_minversion_upgrade"),
                ("package", "commit_upgrade"),
            ]
        } else {
            vec![
                ("package", "authorize_upgrade"),
                ("package", "commit_upgrade"),
            ]
        };
        assert_eq!(move_call_names(pt), expected_calls);
    }
}

#[tokio::test]
async fn public_upgrade_builds_an_enrolled_upgrade() {
    let (builder, cap, gas) =
        test_builder(UpgradePolicy::Additive as u8 | MinVersionState::Enabled as u8);
    let sender = match gas.owner() {
        Owner::AddressOwner(owner) => *owner,
        _ => unreachable!(),
    };
    let data = builder
        .upgrade(
            sender,
            ObjectID::random(),
            vec![vec![1, 2, 3]],
            vec![],
            cap.id(),
            UpgradePolicy::Additive as u8,
            vec![4, 5, 6],
            Some(gas.id()),
            1_000,
        )
        .await
        .unwrap();

    let pt = programmable(data.kind());
    assert_eq!(authorize_policy(pt), UpgradePolicy::Additive as u8);
    assert!(has_mutable_package_config(pt));
    assert!(move_call_names(pt).contains(&("package_config", "record_minversion_upgrade")));
}

#[test]
fn transaction_data_new_upgrade_rejects_an_enrolled_cap() {
    let owner = SuiAddress::random_for_testing_only();
    let cap = upgrade_cap_object(
        UpgradePolicy::Compatible as u8 | MinVersionState::Enabled as u8,
        owner,
    );
    let gas = Object::with_id_owner_for_testing(ObjectID::random(), owner);

    let error = TransactionData::new_upgrade(
        owner,
        gas.compute_object_reference(),
        ObjectID::random(),
        vec![vec![1, 2, 3]],
        vec![SUI_FRAMEWORK_PACKAGE_ID],
        (cap.compute_object_reference(), cap.owner().clone()),
        UpgradePolicy::Compatible as u8 | MinVersionState::Enabled as u8,
        vec![4, 5, 6],
        1_000,
        1,
    )
    .unwrap_err();

    assert!(error.to_string().contains("without PackageConfig"));
}
