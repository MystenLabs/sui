// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

//! Tests for the deny list seal/activate protocol: deny list writes take effect a few
//! consensus commits after they are made, without waiting for an epoch change.

use move_core_types::ident_str;
use move_core_types::language_storage::TypeTag;
use rand::random;
use std::future::Future;
use std::path::PathBuf;
use std::sync::Arc;
use std::time::Duration;
use sui_macros::sim_test;
use sui_protocol_config::ProtocolConfig;
use sui_types::base_types::{EpochId, ObjectID, ObjectRef, SequenceNumber, SuiAddress};
use sui_types::effects::{TransactionEffects, TransactionEffectsAPI};
use sui_types::execution_status::ExecutionErrorKind;
use sui_types::transaction::{CallArg, ObjectArg, SharedObjectMutability, TransactionData};
use sui_types::{
    SUI_ACTIVE_DENY_LIST_OBJECT_ID, SUI_DENY_LIST_OBJECT_ID, SUI_FRAMEWORK_PACKAGE_ID,
};
use test_cluster::{TestCluster, TestClusterBuilder};
use tracing::info;

const DENY_ADDRESS: SuiAddress = SuiAddress::ZERO;

/// A deny list write must take effect within the epoch it was made in, and removing it must
/// take effect within the epoch too. Both are observed through execution of transfers to the
/// denied address.
#[sim_test]
async fn test_deny_list_write_takes_effect_within_epoch() {
    telemetry_subscribers::init_for_testing();
    let _guard = ProtocolConfig::apply_overrides_for_testing(|_, mut config| {
        config.set_enable_deny_list_seal_activate_for_testing(true);
        config
    });
    // Long epochs: the whole test after setup must run inside one epoch.
    let test_env = Arc::new(create_test_env(120_000).await);

    // Before any write, transfers to the address go through.
    let effects = execute_transfer(&test_env).await;
    assert!(effects.status().is_ok(), "{:?}", effects.status());
    assert!(
        read_active_deny_list(&effects),
        "the transfer must record its read of the active deny list in effects: {:?}",
        effects.unchanged_consensus_objects()
    );

    let deny_epoch = execute_deny(&test_env, true).await.executed_epoch();
    let effects = transfer_until(&test_env, |effects| effects.status().is_err()).await;
    let (error_kind, _) = effects.clone().into_status().unwrap_err();
    assert!(
        matches!(
            &error_kind,
            ExecutionErrorKind::AddressDeniedForCoin { address, .. } if *address == DENY_ADDRESS
        ),
        "expected AddressDeniedForCoin, got {error_kind:?}"
    );
    assert_eq!(
        effects.executed_epoch(),
        deny_epoch,
        "the deny must take effect without an epoch change"
    );
    assert!(read_active_deny_list(&effects));

    let undeny_epoch = execute_deny(&test_env, false).await.executed_epoch();
    assert_eq!(undeny_epoch, deny_epoch);
    let effects = transfer_until(&test_env, |effects| effects.status().is_ok()).await;
    assert_eq!(
        effects.executed_epoch(),
        deny_epoch,
        "the removal must take effect without an epoch change"
    );
}

/// Random deny/undeny writes racing with transfers across several epoch changes. Transfers
/// may only fail with the deny list error, and both the seal/activate path within epochs and
/// the flush at epoch end must keep every node executing the same effects (a divergence
/// would surface as a fullnode effects mismatch).
#[sim_test]
async fn test_deny_list_seal_activate_across_epochs() {
    telemetry_subscribers::init_for_testing();
    let _guard = ProtocolConfig::apply_overrides_for_testing(|_, mut config| {
        config.set_enable_deny_list_seal_activate_for_testing(true);
        config
    });
    let test_env = Arc::new(create_test_env(10_000).await);
    let target_epoch = test_env.current_epoch() + 3;

    let deny_env = test_env.clone();
    let deny_thread = tokio::spawn(async move {
        let mut num_writes = 0;
        while deny_env.current_epoch() < target_epoch {
            execute_deny(&deny_env, random()).await;
            num_writes += 1;
        }
        num_writes
    });
    let transfer_env = test_env.clone();
    let transfer_thread = tokio::spawn(async move {
        let mut num_transfers = 0;
        while transfer_env.current_epoch() < target_epoch {
            let effects = execute_transfer(&transfer_env).await;
            if effects.status().is_err() {
                let (error_kind, _) = effects.into_status().unwrap_err();
                assert!(
                    matches!(error_kind, ExecutionErrorKind::AddressDeniedForCoin { .. }),
                    "transfers may only fail with the deny list error, got {error_kind:?}"
                );
            }
            num_transfers += 1;
        }
        num_transfers
    });
    let num_writes = deny_thread.await.unwrap();
    let num_transfers = transfer_thread.await.unwrap();
    assert!(
        num_writes > 5 && num_transfers > 5,
        "{num_writes} writes, {num_transfers} transfers"
    );
}

fn read_active_deny_list(effects: &TransactionEffects) -> bool {
    effects
        .unchanged_consensus_objects()
        .iter()
        .any(|(id, _)| *id == SUI_ACTIVE_DENY_LIST_OBJECT_ID)
}

/// Executes transfers until one satisfies `done`, with a bound well above the activation lag.
async fn transfer_until(
    test_env: &TestEnv,
    done: impl Fn(&TransactionEffects) -> bool,
) -> TransactionEffects {
    for _ in 0..100 {
        let effects = execute_transfer(test_env).await;
        if done(&effects) {
            return effects;
        }
        tokio::time::sleep(Duration::from_millis(200)).await;
    }
    panic!("the deny list write did not take effect in time");
}

async fn execute_deny(test_env: &TestEnv, deny: bool) -> TransactionEffects {
    let effects = test_env
        .execute(|| async {
            let gas = test_env.get_latest_object_ref(&test_env.deny_gas_id).await;
            test_env
                .test_cluster
                .test_transaction_builder_with_gas_object(test_env.regulated_coin_owner, gas)
                .await
                .move_call_with_type_args(
                    SUI_FRAMEWORK_PACKAGE_ID,
                    "coin",
                    if deny {
                        "deny_list_v2_add"
                    } else {
                        "deny_list_v2_remove"
                    },
                    vec![test_env.regulated_coin_type.clone()],
                    vec![
                        CallArg::Object(ObjectArg::SharedObject {
                            id: SUI_DENY_LIST_OBJECT_ID,
                            initial_shared_version: test_env.deny_list_object_init_version,
                            mutability: SharedObjectMutability::Mutable,
                        }),
                        CallArg::Object(ObjectArg::ImmOrOwnedObject(
                            test_env.get_latest_object_ref(&test_env.deny_cap_id).await,
                        )),
                        CallArg::Pure(bcs::to_bytes(&DENY_ADDRESS).unwrap()),
                    ],
                )
                .build()
        })
        .await;
    assert!(effects.status().is_ok(), "{:?}", effects.status());
    info!(
        deny,
        epoch = effects.executed_epoch(),
        "deny list write executed"
    );
    effects
}

/// Splits one unit of the regulated coin and transfers it to `DENY_ADDRESS`.
async fn execute_transfer(test_env: &TestEnv) -> TransactionEffects {
    test_env
        .execute(|| async {
            let gas = test_env
                .get_latest_object_ref(&test_env.transfer_gas_id)
                .await;
            let mut tx_builder = test_env
                .test_cluster
                .test_transaction_builder_with_gas_object(test_env.regulated_coin_owner, gas)
                .await;
            {
                let pt_builder = tx_builder.ptb_builder_mut();
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
            }
            tx_builder.build()
        })
        .await
}

struct TestEnv {
    test_cluster: TestCluster,
    regulated_coin_id: ObjectID,
    regulated_coin_type: TypeTag,
    regulated_coin_owner: SuiAddress,
    deny_cap_id: ObjectID,
    deny_list_object_init_version: SequenceNumber,
    deny_gas_id: ObjectID,
    transfer_gas_id: ObjectID,
}

impl TestEnv {
    async fn get_latest_object_ref(&self, object_id: &ObjectID) -> ObjectRef {
        self.test_cluster
            .get_object_from_fullnode_store(object_id)
            .await
            .unwrap()
            .compute_object_reference()
    }

    fn current_epoch(&self) -> EpochId {
        self.test_cluster
            .fullnode_handle
            .sui_node
            .with(|node| node.state().epoch_store_for_testing().epoch())
    }

    /// Builds and executes a transaction, rebuilding it with fresh object references when
    /// submission fails around an epoch boundary.
    async fn execute<F, Fut>(&self, build: F) -> TransactionEffects
    where
        F: Fn() -> Fut,
        Fut: Future<Output = TransactionData>,
    {
        let mut last_error = None;
        for _ in 0..20 {
            let tx = self.test_cluster.sign_transaction(&build().await).await;
            match self
                .test_cluster
                .wallet
                .execute_transaction_may_fail(tx)
                .await
            {
                Ok(response) => return response.effects,
                Err(e) => {
                    info!("retrying transaction after submission error: {e}");
                    last_error = Some(e);
                }
            }
        }
        panic!("transaction kept failing to submit: {last_error:?}");
    }
}

async fn create_test_env(epoch_duration_ms: u64) -> TestEnv {
    let test_cluster = TestClusterBuilder::new()
        .with_epoch_duration_ms(epoch_duration_ms)
        .with_num_validators(4)
        .build()
        .await;
    // The active deny list is created in the last commit of the first epoch with the flag on
    // (epoch 0 here), so the protocol runs from epoch 1.
    test_cluster.trigger_reconfiguration().await;
    test_cluster.wait_for_epoch_all_nodes(1).await;
    for handle in test_cluster.all_node_handles() {
        handle.with(|node| {
            node.state()
                .epoch_store_for_testing()
                .epoch_start_config()
                .active_deny_list_obj_initial_shared_version()
                .expect("the active deny list must exist from epoch 1");
        });
    }

    // The deny list has already been mutated by the create transaction, so its current
    // version is not its initial shared version.
    let deny_list_object_init_version = test_cluster
        .get_object_from_fullnode_store(&SUI_DENY_LIST_OBJECT_ID)
        .await
        .unwrap()
        .owner()
        .start_version()
        .unwrap();
    let mut path = PathBuf::from(env!("CARGO_MANIFEST_DIR"));
    path.push("tests/move_test_code");
    let tx_data = test_cluster
        .test_transaction_builder()
        .await
        .publish_async(path)
        .await
        .build();
    let effects = test_cluster
        .sign_and_execute_transaction(&tx_data)
        .await
        .effects;
    let mut coin_id = None;
    let mut coin_type = None;
    let mut coin_owner = None;
    let mut deny_cap = None;
    for (reference, owner) in effects.created() {
        let object_id = reference.0;
        let object = test_cluster
            .get_object_from_fullnode_store(&object_id)
            .await
            .unwrap();
        if object.is_package() {
            continue;
        } else if object.is_coin() {
            coin_id = Some(object_id);
            coin_type = object.coin_type_maybe();
            coin_owner = Some(owner.get_address_owner_address().unwrap());
        } else if object.type_().unwrap().is_coin_deny_cap_v2() {
            deny_cap = Some(object_id);
        }
    }
    let regulated_coin_owner = coin_owner.unwrap();
    let mut gas_objects = test_cluster
        .wallet
        .get_gas_objects_owned_by_address(regulated_coin_owner, None)
        .await
        .unwrap()
        .into_iter()
        .map(|gas| gas.0);
    let deny_gas_id = gas_objects.next().unwrap();
    let transfer_gas_id = gas_objects.next().unwrap();
    TestEnv {
        test_cluster,
        regulated_coin_id: coin_id.unwrap(),
        regulated_coin_type: coin_type.unwrap(),
        regulated_coin_owner,
        deny_cap_id: deny_cap.unwrap(),
        deny_list_object_init_version,
        deny_gas_id,
        transfer_gas_id,
    }
}
