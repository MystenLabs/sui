// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

use crate::authority::authority_tests::send_and_confirm_transaction_;
use crate::authority::move_integration_tests::build_and_try_publish_test_package;
use crate::authority::test_authority_builder::TestAuthorityBuilder;
use crate::authority::AuthorityState;
use itertools::Itertools;
use move_core_types::ident_str;
use move_core_types::language_storage::{StructTag, TypeTag};
use std::collections::BTreeMap;
use std::sync::Arc;
use sui_protocol_config::ProtocolConfig;
use sui_test_transaction_builder::TestTransactionBuilder;
use sui_types::base_types::{dbg_addr, ObjectID, ObjectRef, SequenceNumber, SuiAddress};
use sui_types::crypto::{get_account_key_pair, AccountKeyPair};
use sui_types::effects::{TransactionEffectsAPI, UnchangedSharedKind};
use sui_types::object::Object;
use sui_types::programmable_transaction_builder::ProgrammableTransactionBuilder;
use sui_types::transaction::{CallArg, ObjectArg, Transaction, TEST_ONLY_GAS_UNIT_FOR_PUBLISH};
use sui_types::{Identifier, SUI_DENY_LIST_OBJECT_ID, SUI_FRAMEWORK_PACKAGE_ID};

const DENY_ADDRESS: SuiAddress = SuiAddress::ZERO;

enum DenyListActivity {
    Transfer,
    Mutate,
    ChangeEpoch,
}

async fn runner(activities: Vec<DenyListActivity>) {
    let mut env = TestEnv::new_authority_and_publish("coin_deny_list_v2", None).await;
    let mut epoch_sequence_numbers: BTreeMap<u64, Vec<_>> = BTreeMap::new();
    let mut epoch = 0;
    for activity in activities {
        match activity {
            DenyListActivity::Transfer => {
                let tx = env.create_native_transfer_tx().await;
                let effects = send_and_confirm_transaction_(&env.authority, None, tx, true)
                    .await
                    .unwrap()
                    .1;
                let Some((_, UnchangedSharedKind::PerEpochConfigWithSeqno(pseqno))) = effects
                    .unchanged_shared_objects()
                    .into_iter()
                    .find(|id| id.0 == SUI_DENY_LIST_OBJECT_ID)
                else {
                    panic!(
                        "Invalid unchanged shared object output for effects: {:?}",
                        effects.unchanged_shared_objects()
                    );
                };
                epoch_sequence_numbers
                    .entry(epoch)
                    .or_default()
                    .push(pseqno);
            }
            DenyListActivity::Mutate => {
                let tx = env.create_deny_list_mutation().await;
                send_and_confirm_transaction_(&env.authority, None, tx, true)
                    .await
                    .unwrap();
            }
            DenyListActivity::ChangeEpoch => {
                env.authority.reconfigure_for_testing().await;
                epoch += 1;
            }
        }
    }

    // Assert that all sequence numbers in each epoch are equal.
    for (epoch, seqnos) in epoch_sequence_numbers {
        assert!(
            seqnos.into_iter().all_equal(),
            "Sequence numbers in epoch {} are not equal",
            epoch
        );
    }
}

#[tokio::test]
async fn test_epoch_stable_sequence_numbers_mutation_first() {
    use DenyListActivity::*;
    runner(vec![
        Mutate,
        Transfer,
        Transfer,
        ChangeEpoch,
        Mutate,
        Transfer,
        Mutate,
        Transfer,
        Mutate,
        Transfer,
    ])
    .await;
}

#[tokio::test]
async fn test_epoch_stable_sequence_numbers_use_then_mutate() {
    use DenyListActivity::*;
    runner(vec![
        Transfer,
        Transfer,
        Mutate,
        Transfer,
        Transfer,
        ChangeEpoch,
        Transfer,
        Transfer,
        Mutate,
        Transfer,
        Transfer,
        Mutate,
        Transfer,
        Transfer,
        Mutate,
        Transfer,
        Transfer,
    ])
    .await;
}

#[tokio::test]
async fn test_config_use_before_change_not_input_object() {
    let mut protocol_config = ProtocolConfig::get_for_max_version_UNSAFE();
    protocol_config.set_include_epoch_stable_sequence_number_in_effects_for_testing(false);
    let mut env =
        TestEnv::new_authority_and_publish("coin_deny_list_v2", Some(protocol_config)).await;
    let tx = env.create_native_transfer_tx().await;
    // assert 0x403 is not in the inputs
    assert!(!tx
        .shared_input_objects()
        .any(|obj| { obj.id() == SUI_DENY_LIST_OBJECT_ID }));
    let effects = send_and_confirm_transaction_(&env.authority, None, tx, true)
        .await
        .unwrap()
        .1;
    let unchanged_shared_objects = effects.unchanged_shared_objects();
    assert_eq!(unchanged_shared_objects.len(), 1);
    assert_eq!(unchanged_shared_objects[0].0, SUI_DENY_LIST_OBJECT_ID);
    assert_eq!(
        unchanged_shared_objects[0].1,
        UnchangedSharedKind::PerEpochConfigDEPRECATED
    );
}

#[tokio::test]
async fn test_config_use_before_change_deny_list_in_input_object() {
    let mut protocol_config = ProtocolConfig::get_for_max_version_UNSAFE();
    protocol_config.set_include_epoch_stable_sequence_number_in_effects_for_testing(false);
    let mut env =
        TestEnv::new_authority_and_publish("coin_deny_list_v2", Some(protocol_config)).await;
    {
        let add_tx = env.create_deny_list_mutation().await;
        send_and_confirm_transaction_(&env.authority, None, add_tx, true)
            .await
            .unwrap();
    }
    let tx = env.transfer_tx_with_deny_list_present().await;
    // Assert 0x403 is in the inputs
    assert!(tx
        .shared_input_objects()
        .any(|obj| { obj.id() == SUI_DENY_LIST_OBJECT_ID }));
    let effects = send_and_confirm_transaction_(&env.authority, None, tx, true)
        .await
        .unwrap()
        .1;
    // 0x403 will not show up in the unchanged shared objects since it was a transaction input
    let unchanged_shared_objects = effects.unchanged_shared_objects();
    assert!(unchanged_shared_objects.is_empty());
}

#[tokio::test]
async fn test_config_use_after_change_deny_list_in_input_object() {
    let protocol_config = ProtocolConfig::get_for_max_version_UNSAFE();
    let mut env =
        TestEnv::new_authority_and_publish("coin_deny_list_v2", Some(protocol_config)).await;
    {
        let add_tx = env.create_deny_list_mutation().await;
        send_and_confirm_transaction_(&env.authority, None, add_tx, true)
            .await
            .unwrap();
    }
    let tx = env.transfer_tx_with_deny_list_present().await;
    // Assert 0x403 is in the inputs
    assert!(tx
        .shared_input_objects()
        .any(|obj| { obj.id() == SUI_DENY_LIST_OBJECT_ID }));
    let effects = send_and_confirm_transaction_(&env.authority, None, tx, true)
        .await
        .unwrap()
        .1;
    // 0x403 will not show up in the unchanged shared objects since it was a transaction input
    let unchanged_shared_objects = effects.unchanged_shared_objects();
    assert_eq!(unchanged_shared_objects.len(), 1);
    // It's a different address (the dynamic field address) that we will be adding here. So make
    // sure it's not the deny list object ID to avoid possible false positives
    assert_ne!(unchanged_shared_objects[0].0, SUI_DENY_LIST_OBJECT_ID);
}

struct TestEnv {
    authority: Arc<AuthorityState>,
    sender: SuiAddress,
    keypair: AccountKeyPair,
    gas_object_id: ObjectID,
    regulated_coin_type: TypeTag,
    regulated_coin_id: ObjectID,
    deny_cap_id: ObjectID,
    deny_list_object_init_version: SequenceNumber,
    package_id: ObjectID,
}

impl TestEnv {
    async fn get_latest_object_ref(&self, id: &ObjectID) -> ObjectRef {
        self.authority
            .get_object(id)
            .await
            .unwrap()
            .compute_object_reference()
    }

    async fn create_deny_list_mutation(&mut self) -> Transaction {
        let deny_cap_obj_ref = self.get_latest_object_ref(&self.deny_cap_id).await;
        let regulated_coin_type = TypeTag::Struct(Box::new(StructTag {
            address: self.package_id.into(),
            module: ident_str!("regulated_coin").to_owned(),
            name: ident_str!("REGULATED_COIN").to_owned(),
            type_params: vec![],
        }));
        let deny_address = dbg_addr(2);
        TestTransactionBuilder::new(
            self.sender,
            self.get_latest_object_ref(&self.gas_object_id).await,
            self.authority.reference_gas_price_for_testing().unwrap(),
        )
        .move_call(
            SUI_FRAMEWORK_PACKAGE_ID,
            "coin",
            "deny_list_v2_add",
            vec![
                CallArg::Object(ObjectArg::SharedObject {
                    id: SUI_DENY_LIST_OBJECT_ID,
                    initial_shared_version: self.deny_list_object_init_version,
                    mutable: true,
                }),
                CallArg::Object(ObjectArg::ImmOrOwnedObject(deny_cap_obj_ref)),
                CallArg::Pure(bcs::to_bytes(&deny_address).unwrap()),
            ],
        )
        .with_type_args(vec![regulated_coin_type.clone()])
        .build_and_sign(&self.keypair)
    }

    async fn transfer_tx_with_deny_list_present(&mut self) -> Transaction {
        let deny_address = dbg_addr(2);
        let pt = {
            let mut builder = ProgrammableTransactionBuilder::new();
            builder
                .move_call(
                    SUI_FRAMEWORK_PACKAGE_ID,
                    Identifier::new("coin").unwrap(),
                    Identifier::new("deny_list_v2_contains_current_epoch").unwrap(),
                    vec![self.regulated_coin_type.clone()],
                    vec![
                        CallArg::Object(ObjectArg::SharedObject {
                            id: SUI_DENY_LIST_OBJECT_ID,
                            initial_shared_version: self.deny_list_object_init_version,
                            mutable: true,
                        }),
                        CallArg::Pure(bcs::to_bytes(&deny_address).unwrap()),
                    ],
                )
                .unwrap();

            builder
                .move_call(
                    SUI_FRAMEWORK_PACKAGE_ID,
                    Identifier::new("pay").unwrap(),
                    Identifier::new("split_and_transfer").unwrap(),
                    vec![self.regulated_coin_type.clone()],
                    vec![
                        CallArg::Object(ObjectArg::ImmOrOwnedObject(
                            self.get_latest_object_ref(&self.regulated_coin_id).await,
                        )),
                        CallArg::Pure(bcs::to_bytes(&1u64).unwrap()),
                        CallArg::Pure(bcs::to_bytes(&DENY_ADDRESS).unwrap()),
                    ],
                )
                .unwrap();
            builder.finish()
        };
        TestTransactionBuilder::new(
            self.sender,
            self.get_latest_object_ref(&self.gas_object_id).await,
            self.authority.reference_gas_price_for_testing().unwrap(),
        )
        .programmable(pt)
        .build_and_sign(&self.keypair)
    }

    async fn create_native_transfer_tx(&mut self) -> Transaction {
        TestTransactionBuilder::new(
            self.sender,
            self.get_latest_object_ref(&self.gas_object_id).await,
            self.authority.reference_gas_price_for_testing().unwrap(),
        )
        .move_call(
            SUI_FRAMEWORK_PACKAGE_ID,
            "pay",
            "split_and_transfer",
            vec![
                CallArg::Object(ObjectArg::ImmOrOwnedObject(
                    self.get_latest_object_ref(&self.regulated_coin_id).await,
                )),
                CallArg::Pure(bcs::to_bytes(&1u64).unwrap()),
                CallArg::Pure(bcs::to_bytes(&DENY_ADDRESS).unwrap()),
            ],
        )
        .with_type_args(vec![self.regulated_coin_type.clone()])
        .build_and_sign(&self.keypair)
    }

    async fn new_authority_and_publish(
        path: &str,
        protocol_config: Option<ProtocolConfig>,
    ) -> Self {
        let (sender, keypair) = get_account_key_pair();
        let gas_object = Object::with_owner_for_testing(sender);
        let gas_object_id = gas_object.id();
        let mut authority = TestAuthorityBuilder::new();
        if let Some(config) = protocol_config {
            authority = authority.with_protocol_config(config);
        }

        let authority = authority.with_starting_objects(&[gas_object]).build().await;
        let rgp = authority.reference_gas_price_for_testing().unwrap();
        let (_, effects) = build_and_try_publish_test_package(
            &authority,
            &sender,
            &keypair,
            &gas_object_id,
            path,
            TEST_ONLY_GAS_UNIT_FOR_PUBLISH * rgp,
            rgp,
            false,
        )
        .await;
        let deny_list_object_init_version = authority
            .get_object(&SUI_DENY_LIST_OBJECT_ID)
            .await
            .unwrap()
            .version();
        let mut coin_id = None;
        let mut coin_type = None;
        let mut deny_cap = None;
        let mut package_id = None;
        for created in effects.created() {
            let object_id = created.0 .0;
            let object = authority.get_object(&object_id).await.unwrap();
            if object.is_package() {
                package_id = Some(object_id);
                continue;
            } else if object.is_coin() {
                coin_id = Some(object_id);
                coin_type = object.coin_type_maybe();
            } else if object.type_().unwrap().is_coin_deny_cap_v2() {
                deny_cap = Some(object_id);
            }
        }
        TestEnv {
            authority,
            sender,
            keypair,
            gas_object_id,
            regulated_coin_id: coin_id.unwrap(),
            regulated_coin_type: coin_type.unwrap(),
            deny_cap_id: deny_cap.unwrap(),
            deny_list_object_init_version,
            package_id: package_id.unwrap(),
        }
    }
}
