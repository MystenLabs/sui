// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

use move_core_types::language_storage::TypeTag;
use rand::random;
use std::collections::{HashMap, HashSet};
use std::path::PathBuf;
use std::sync::Arc;
use std::time::Duration;
use sui_json_rpc_types::SuiTransactionBlockEffectsAPI;
use sui_macros::sim_test;
use sui_types::base_types::{EpochId, ObjectID, ObjectRef, SuiAddress};
use sui_types::transaction::{CallArg, ObjectArg};
use sui_types::{SUI_DENY_LIST_OBJECT_ID, SUI_FRAMEWORK_PACKAGE_ID};
use test_cluster::{TestCluster, TestClusterBuilder};
use tokio::sync::RwLock;

const DENY_ADDRESS: SuiAddress = SuiAddress::ZERO;

#[sim_test]
async fn per_epoch_config_stress_test() {
    let test_env = Arc::new(create_test_env().await);
    let handle1 = {
        let test_env = test_env.clone();
        tokio::spawn(async move { run_transfer_thread(test_env).await })
    };
    let handle2 = {
        let test_env = test_env.clone();
        tokio::spawn(async move { run_admin_thread(test_env).await })
    };
    let _ = tokio::time::timeout(Duration::from_secs(120), async {
        tokio::try_join!(handle1, handle2)
    })
    .await;
}

async fn run_admin_thread(test_env: Arc<TestEnv>) {
    let deny_list_object_init_version = test_env
        .get_latest_object_ref(&SUI_DENY_LIST_OBJECT_ID)
        .await
        .1;
    loop {
        let deny: bool = random();
        let tx_data = test_env
            .test_cluster
            .test_transaction_builder()
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
                        initial_shared_version: deny_list_object_init_version,
                        mutable: true,
                    }),
                    CallArg::Object(ObjectArg::ImmOrOwnedObject(
                        test_env.get_latest_object_ref(&test_env.deny_cap_id).await,
                    )),
                    CallArg::Pure(bcs::to_bytes(&DENY_ADDRESS).unwrap()),
                ],
            )
            .with_type_args(vec![test_env.regulated_coin_type.clone()])
            .build();
        let tx = test_env.test_cluster.sign_transaction(&tx_data);
        let effects = test_env
            .test_cluster
            .wallet
            .execute_transaction_may_fail(tx)
            .await
            .unwrap()
            .effects
            .unwrap();
        let executed_epoch = effects.executed_epoch();
        let already_denied = test_env
            .is_denied_at_epoch
            .read()
            .await
            .contains(&executed_epoch);
        if !already_denied && deny {
            assert!(effects.status().is_err());
        } else {
            assert!(effects.status().is_ok());
        }
        if deny {
            test_env
                .is_denied_at_epoch
                .write()
                .await
                .insert(executed_epoch);
        } else {
            test_env
                .is_denied_at_epoch
                .write()
                .await
                .remove(&executed_epoch);
        }
    }
}

async fn run_transfer_thread(test_env: Arc<TestEnv>) {
    tokio::time::sleep(Duration::from_secs(4)).await;
    println!("d");
}

struct TestEnv {
    test_cluster: TestCluster,
    package_id: ObjectID,
    regulated_coin_id: ObjectID,
    regulated_coin_type: TypeTag,
    regulated_coin_owner: SuiAddress,
    deny_cap_id: ObjectID,

    is_denied_at_epoch: RwLock<HashSet<EpochId>>,
}

impl TestEnv {
    async fn get_latest_object_ref(&self, object_id: &ObjectID) -> ObjectRef {
        self.test_cluster
            .get_object_from_fullnode_store(&object_id)
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
    let mut package_id = None;
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
            package_id = Some(object_id);
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
        package_id: package_id.unwrap(),
        regulated_coin_id: coin_id.unwrap(),
        regulated_coin_type: coin_type.unwrap(),
        regulated_coin_owner: coin_owner.unwrap(),
        deny_cap_id: deny_cap.unwrap(),
    }
}
