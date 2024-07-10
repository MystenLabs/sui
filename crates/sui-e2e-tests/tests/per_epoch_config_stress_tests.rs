// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

use move_core_types::ident_str;
use move_core_types::language_storage::TypeTag;
use rand::random;
use std::future::Future;
use std::path::PathBuf;
use std::sync::Arc;
use std::time::Duration;
use sui_json_rpc_types::SuiTransactionBlockEffectsAPI;
use sui_macros::sim_test;
use sui_types::base_types::SequenceNumber;
use sui_types::base_types::{EpochId, ObjectID, ObjectRef, SuiAddress};
use sui_types::programmable_transaction_builder::ProgrammableTransactionBuilder;
use sui_types::transaction::{CallArg, ObjectArg, TransactionData};
use sui_types::{SUI_DENY_LIST_OBJECT_ID, SUI_FRAMEWORK_PACKAGE_ID};
use test_cluster::{TestCluster, TestClusterBuilder};

const DENY_ADDRESS: SuiAddress = SuiAddress::ZERO;

#[sim_test]
async fn per_epoch_config_stress_test() {
    let test_env = Arc::new(create_test_env().await);
    let target_epoch = 10;
    let mut gas_objects = test_env
        .test_cluster
        .wallet
        .get_all_gas_objects_owned_by_address(test_env.regulated_coin_owner)
        .await
        .unwrap();
    let gas1 = gas_objects.pop().unwrap();
    let gas2 = gas_objects.pop().unwrap();
    let handle1 = {
        let test_env = test_env.clone();
        tokio::spawn(async move {
            run_thread(test_env, target_epoch, gas1.0, create_transfer_tx, true).await
        })
    };
    let handle2 = {
        let test_env = test_env.clone();
        tokio::spawn(async move {
            run_thread(test_env, target_epoch, gas2.0, create_deny_tx, false).await
        })
    };
    tokio::time::timeout(Duration::from_secs(600), async {
        tokio::try_join!(handle1, handle2)
    })
    .await
    .unwrap()
    .unwrap();
}

async fn run_thread<F, Fut>(
    test_env: Arc<TestEnv>,
    target_epoch: EpochId,
    gas_id: ObjectID,
    tx_creation_func: F,
    tx_may_fail: bool,
) where
    F: Fn(Arc<TestEnv>, ObjectRef) -> Fut,
    Fut: Future<Output = TransactionData>,
{
    let mut num_tx_succeeded = 0;
    let mut num_tx_failed = 0;
    loop {
        let gas = test_env.get_latest_object_ref(&gas_id).await;
        let tx_data = tx_creation_func(test_env.clone(), gas).await;
        let tx = test_env.test_cluster.sign_transaction(&tx_data);
        let Ok(effects) = test_env
            .test_cluster
            .wallet
            .execute_transaction_may_fail(tx)
            .await
            .map(|r| r.effects.unwrap())
        else {
            // When epochs are short, it is possible that some transactions
            // keep getting sent at epoch boundaries and timeout eventually.
            continue;
        };
        if effects.status().is_ok() {
            num_tx_succeeded += 1;
        } else {
            num_tx_failed += 1;
        }
        let executed_epoch = effects.executed_epoch();
        if executed_epoch >= target_epoch {
            break;
        }
    }
    if !tx_may_fail {
        assert_eq!(num_tx_failed, 0);
    }
    assert!(num_tx_succeeded + num_tx_failed > 5);
}

async fn create_deny_tx(test_env: Arc<TestEnv>, gas: ObjectRef) -> TransactionData {
    let deny: bool = random();
    test_env
        .test_cluster
        .test_transaction_builder_with_gas_object(test_env.regulated_coin_owner, gas)
        .await
        .move_call(
            SUI_FRAMEWORK_PACKAGE_ID,
            "coin",
            if deny {
                "deny_list_v2_add"
            } else {
                "deny_list_v2_remove"
            },
            vec![
                CallArg::Object(ObjectArg::SharedObject {
                    id: SUI_DENY_LIST_OBJECT_ID,
                    initial_shared_version: test_env.deny_list_object_init_version,
                    mutable: true,
                }),
                CallArg::Object(ObjectArg::ImmOrOwnedObject(
                    test_env.get_latest_object_ref(&test_env.deny_cap_id).await,
                )),
                CallArg::Pure(bcs::to_bytes(&DENY_ADDRESS).unwrap()),
            ],
        )
        .with_type_args(vec![test_env.regulated_coin_type.clone()])
        .build()
}

async fn create_transfer_tx(test_env: Arc<TestEnv>, gas: ObjectRef) -> TransactionData {
    let use_move: bool = random();
    if use_move {
        create_move_transfer_tx(test_env, gas).await
    } else {
        create_native_transfer_tx(test_env, gas).await
    }
}

async fn create_move_transfer_tx(test_env: Arc<TestEnv>, gas: ObjectRef) -> TransactionData {
    test_env
        .test_cluster
        .test_transaction_builder_with_gas_object(test_env.regulated_coin_owner, gas)
        .await
        .move_call(
            SUI_FRAMEWORK_PACKAGE_ID,
            "pay",
            "split_and_transfer",
            vec![
                CallArg::Object(ObjectArg::ImmOrOwnedObject(
                    test_env
                        .get_latest_object_ref(&test_env.regulated_coin_id)
                        .await,
                )),
                CallArg::Pure(bcs::to_bytes(&1u64).unwrap()),
                CallArg::Pure(bcs::to_bytes(&DENY_ADDRESS).unwrap()),
            ],
        )
        .with_type_args(vec![test_env.regulated_coin_type.clone()])
        .build()
}

async fn create_native_transfer_tx(test_env: Arc<TestEnv>, gas: ObjectRef) -> TransactionData {
    let mut pt_builder = ProgrammableTransactionBuilder::new();
    let coin_input = pt_builder
        .obj(ObjectArg::ImmOrOwnedObject(
            test_env
                .get_latest_object_ref(&test_env.regulated_coin_id)
                .await,
        ))
        .unwrap();
    let amount_input = pt_builder.pure(1u64).unwrap();
    let split_coin = pt_builder.programmable_move_call(
        SUI_FRAMEWORK_PACKAGE_ID,
        ident_str!("coin").to_owned(),
        ident_str!("split").to_owned(),
        vec![test_env.regulated_coin_type.clone()],
        vec![coin_input, amount_input],
    );
    pt_builder.transfer_arg(DENY_ADDRESS, split_coin);
    let pt = pt_builder.finish();
    test_env
        .test_cluster
        .test_transaction_builder_with_gas_object(test_env.regulated_coin_owner, gas)
        .await
        .programmable(pt)
        .build()
}

struct TestEnv {
    test_cluster: TestCluster,
    regulated_coin_id: ObjectID,
    regulated_coin_type: TypeTag,
    regulated_coin_owner: SuiAddress,
    deny_cap_id: ObjectID,
    deny_list_object_init_version: SequenceNumber,
}

impl TestEnv {
    async fn get_latest_object_ref(&self, object_id: &ObjectID) -> ObjectRef {
        self.test_cluster
            .get_object_from_fullnode_store(object_id)
            .await
            .unwrap()
            .compute_object_reference()
    }
}

async fn create_test_env() -> TestEnv {
    let test_cluster = TestClusterBuilder::new()
        .with_epoch_duration_ms(1000)
        .with_num_validators(5)
        .build()
        .await;
    let deny_list_object_init_version = test_cluster
        .get_object_from_fullnode_store(&SUI_DENY_LIST_OBJECT_ID)
        .await
        .unwrap()
        .version();
    let mut path = PathBuf::from(env!("CARGO_MANIFEST_DIR"));
    path.push("tests/move_test_code");
    let tx_data = test_cluster
        .test_transaction_builder()
        .await
        .publish(path)
        .build();
    let effects = test_cluster
        .sign_and_execute_transaction(&tx_data)
        .await
        .effects
        .unwrap();
    let mut coin_id = None;
    let mut coin_type = None;
    let mut coin_owner = None;
    let mut deny_cap = None;
    for created in effects.created() {
        let object_id = created.reference.object_id;
        let object = test_cluster
            .get_object_from_fullnode_store(&object_id)
            .await
            .unwrap();
        if object.is_package() {
            continue;
        } else if object.is_coin() {
            coin_id = Some(object_id);
            coin_type = object.coin_type_maybe();
            coin_owner = Some(created.owner.get_address_owner_address().unwrap());
        } else if object.type_().unwrap().is_coin_deny_cap_v2() {
            deny_cap = Some(object_id);
        }
    }
    TestEnv {
        test_cluster,
        regulated_coin_id: coin_id.unwrap(),
        regulated_coin_type: coin_type.unwrap(),
        regulated_coin_owner: coin_owner.unwrap(),
        deny_cap_id: deny_cap.unwrap(),
        deny_list_object_init_version,
    }
}
