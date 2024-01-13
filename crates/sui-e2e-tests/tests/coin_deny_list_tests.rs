// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

use move_core_types::ident_str;
use move_core_types::language_storage::TypeTag;
use std::path::PathBuf;
use sui_core::authority::epoch_start_configuration::EpochStartConfigTrait;
use sui_json_rpc_types::SuiTransactionBlockKind;
use sui_json_rpc_types::{SuiTransactionBlockDataAPI, SuiTransactionBlockResponseOptions};
use sui_json_rpc_types::{SuiTransactionBlockEffectsAPI, SuiTransactionBlockResponse};
use sui_macros::sim_test;
use sui_types::base_types::{ObjectID, SuiAddress};
use sui_types::coin::COIN_MODULE_NAME;
use sui_types::deny_list::RegulatedCoinMetadata;
use sui_types::deny_list::{
    get_coin_deny_list, get_deny_list_obj_initial_shared_version, get_deny_list_root_object,
    CoinDenyCap, DenyList,
};
use sui_types::error::UserInputError;
use sui_types::id::UID;
use sui_types::programmable_transaction_builder::ProgrammableTransactionBuilder;
use sui_types::storage::ObjectStore;
use sui_types::transaction::{CallArg, ObjectArg};
use sui_types::{SUI_DENY_LIST_OBJECT_ID, SUI_FRAMEWORK_PACKAGE_ID};
use test_cluster::{TestCluster, TestClusterBuilder};
use tracing::debug;

#[sim_test]
async fn test_coin_deny_list_creation() {
    let test_cluster = TestClusterBuilder::new()
        .with_protocol_version(34.into())
        .with_epoch_duration_ms(10000)
        .build()
        .await;
    for handle in test_cluster.all_node_handles() {
        handle.with(|node| {
            assert!(get_deny_list_obj_initial_shared_version(&node.state().database).is_none());
            assert!(!node
                .state()
                .epoch_store_for_testing()
                .coin_deny_list_state_exists());
        });
    }
    test_cluster.wait_for_epoch_all_nodes(2).await;
    let mut prev_tx = None;
    for handle in test_cluster.all_node_handles() {
        handle.with(|node| {
            assert_eq!(
                node.state()
                    .epoch_store_for_testing()
                    .protocol_version()
                    .as_u64(),
                35
            );
            let version = node
                .state()
                .epoch_store_for_testing()
                .epoch_start_config()
                .coin_deny_list_obj_initial_shared_version()
                .unwrap();

            let deny_list_object = get_deny_list_root_object(&node.state().database).unwrap();
            assert_eq!(deny_list_object.version(), version);
            assert!(deny_list_object.owner.is_shared());
            let deny_list: DenyList = deny_list_object.to_rust().unwrap();
            assert_eq!(deny_list.id, UID::new(SUI_DENY_LIST_OBJECT_ID));
            assert_eq!(deny_list.lists.size, 1);

            if let Some(prev_tx) = prev_tx {
                assert_eq!(deny_list_object.previous_transaction, prev_tx);
            } else {
                prev_tx = Some(deny_list_object.previous_transaction);
            }

            let coin_deny_list = get_coin_deny_list(&node.state().database).unwrap();
            assert_eq!(coin_deny_list.denied_count.size, 0);
            assert_eq!(coin_deny_list.denied_addresses.size, 0);
        });
    }
    let prev_tx = prev_tx.unwrap();
    let tx = test_cluster
        .fullnode_handle
        .sui_client
        .read_api()
        .get_transaction_with_options(prev_tx, SuiTransactionBlockResponseOptions::full_content())
        .await
        .unwrap()
        .transaction
        .unwrap();
    assert!(matches!(
        tx.data.transaction(),
        SuiTransactionBlockKind::EndOfEpochTransaction(_)
    ));
    test_cluster.wait_for_epoch_all_nodes(3).await;
    // Check that we are not re-creating the same object again.
    for handle in test_cluster.all_node_handles() {
        handle.with(|node| {
            assert_eq!(
                node.state()
                    .database
                    .get_object(&SUI_DENY_LIST_OBJECT_ID)
                    .unwrap()
                    .unwrap()
                    .previous_transaction,
                prev_tx
            );
        });
    }
}

#[sim_test]
async fn test_coin_deny_transfer() {
    // This test creates a new coin and denies the sender address for the coin.
    // A transfer transaction is subsequently denied.
    // The sender address is then undenied and the transfer transaction is allowed.

    let mut test_context = TestContext::new().await;
    // Deny the sender address for the new coin.
    test_context
        .call_deny_list_api(test_context.new_coin_owner, DenyAction::Deny)
        .await;
    // After the address is denied, sending the coin should fail to sign.
    let transfer_result = test_context
        .transfer_new_coin(test_context.test_cluster.get_address_2())
        .await;
    let expected_error = UserInputError::AddressDeniedForCoin {
        address: test_context.new_coin_owner,
        coin_type: test_context
            .get_new_coin_type()
            .await
            .to_canonical_string(false),
    };
    assert!(transfer_result
        .unwrap_err()
        .to_string()
        .contains(&expected_error.to_string()));

    // Sending SUI coin should still work.
    let tx_data = test_context
        .test_cluster
        .test_transaction_builder_with_sender(test_context.new_coin_owner)
        .await
        .transfer_sui(Some(1), test_context.test_cluster.get_address_2())
        .build();
    test_context
        .test_cluster
        .sign_and_execute_transaction(&tx_data)
        .await;

    // Undeny the address.
    test_context
        .call_deny_list_api(test_context.new_coin_owner, DenyAction::Undeny)
        .await;

    // After the address is undenied, sending the coin should work now.
    assert!(test_context
        .transfer_new_coin(test_context.test_cluster.get_address_2())
        .await
        .is_ok_and(|r| r.effects.unwrap().status().is_ok()));
}

#[sim_test]
async fn test_coin_deny_move_call() {
    // This test creates a new coin and denies the sender address for the coin.
    // A Move call transaction that uses the new coin from the denied address is subsequently denied.
    // The sender address is then undenied and the Move call transaction is allowed.

    let test_context = TestContext::new().await;
    // Deny the sender address for the new coin.
    test_context
        .call_deny_list_api(test_context.new_coin_owner, DenyAction::Deny)
        .await;

    // After the address is denied, using the coin in a Move call should fail to sign.
    let mut pt = ProgrammableTransactionBuilder::new();
    let object_arg = pt
        .obj(ObjectArg::ImmOrOwnedObject(
            test_context
                .test_cluster
                .get_latest_object_ref(&test_context.new_coin_id)
                .await,
        ))
        .unwrap();
    let pure_arg = pt.pure(1u64).unwrap();
    let split_result = pt.programmable_move_call(
        SUI_FRAMEWORK_PACKAGE_ID,
        ident_str!("coin").to_owned(),
        ident_str!("split").to_owned(),
        vec![test_context.get_new_coin_type().await],
        vec![object_arg, pure_arg],
    );
    pt.transfer_arg(test_context.test_cluster.get_address_2(), split_result);
    let tx_data = test_context
        .test_cluster
        .test_transaction_builder_with_sender(test_context.new_coin_owner)
        .await
        .programmable(pt.finish())
        .build();
    let tx = test_context.test_cluster.sign_transaction(&tx_data);
    let result = test_context
        .test_cluster
        .wallet
        .execute_transaction_may_fail(tx.clone())
        .await;
    let expected_error = UserInputError::AddressDeniedForCoin {
        address: test_context.new_coin_owner,
        coin_type: test_context
            .get_new_coin_type()
            .await
            .to_canonical_string(false),
    };
    assert!(result
        .unwrap_err()
        .to_string()
        .contains(&expected_error.to_string()));

    // Undeny the address.
    test_context
        .call_deny_list_api(test_context.new_coin_owner, DenyAction::Undeny)
        .await;

    // After the address is undenied, the same transaction should work now.
    test_context.test_cluster.execute_transaction(tx).await;
}

#[sim_test]
async fn test_coin_deny_tto_receiving() {
    // This test creates a new coin and denies the sender address for the coin.
    // A transaction that attempts to receive the new coin from the denied address is subsequently denied.
    // The sender address is then undenied and the transaction is allowed.

    let mut test_context = TestContext::new().await;
    let new_sender = test_context.test_cluster.get_address_2();
    // Deny this new address for the new coin.
    test_context
        .call_deny_list_api(new_sender, DenyAction::Deny)
        .await;

    // Even though the new address is denied, it can still call the contract of the coin package to do other things.
    let package_id = test_context.get_new_coin_package_id().await;
    let tx_data = test_context
        .test_cluster
        .test_transaction_builder_with_sender(new_sender)
        .await
        .move_call(package_id, "regulated_coin", "new_wallet", vec![])
        .build();
    let wallet_oref = test_context
        .test_cluster
        .sign_and_execute_transaction(&tx_data)
        .await
        .effects
        .unwrap()
        .created()[0]
        .reference
        .to_object_ref();
    test_context
        .transfer_new_coin(wallet_oref.0.into())
        .await
        .unwrap();

    // After the address is denied, trying to receive the coin in a Move call should fail to sign.
    let mut pt = ProgrammableTransactionBuilder::new();
    pt.move_call(
        package_id,
        ident_str!("regulated_coin").to_owned(),
        ident_str!("receive_coin").to_owned(),
        vec![],
        vec![
            CallArg::Object(ObjectArg::ImmOrOwnedObject(wallet_oref)),
            CallArg::Object(ObjectArg::Receiving(
                test_context
                    .test_cluster
                    .get_latest_object_ref(&test_context.new_coin_id)
                    .await,
            )),
        ],
    )
    .unwrap();
    let tx_data = test_context
        .test_cluster
        .test_transaction_builder_with_sender(new_sender)
        .await
        .programmable(pt.finish())
        .build();
    let tx = test_context.test_cluster.sign_transaction(&tx_data);
    let result = test_context
        .test_cluster
        .wallet
        .execute_transaction_may_fail(tx.clone())
        .await;
    let expected_error = UserInputError::AddressDeniedForCoin {
        address: new_sender,
        coin_type: test_context
            .get_new_coin_type()
            .await
            .to_canonical_string(false),
    };
    assert!(result
        .unwrap_err()
        .to_string()
        .contains(&expected_error.to_string()));

    // Undeny the address.
    test_context
        .call_deny_list_api(new_sender, DenyAction::Undeny)
        .await;

    // After the address is undenied, the same transaction should work now.
    test_context.test_cluster.execute_transaction(tx).await;
}

struct TestContext {
    test_cluster: TestCluster,
    new_coin_id: ObjectID,
    new_coin_owner: SuiAddress,
    deny_cap_object_id: ObjectID,
    deny_cap_object_owner: SuiAddress,
}

#[derive(Debug)]
enum DenyAction {
    Deny,
    Undeny,
}

impl TestContext {
    // Returns the test cluster and the deny cap.
    async fn new() -> Self {
        let test_cluster = TestClusterBuilder::new().build().await;
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
        let mut new_coin_object = None;
        let mut deny_cap_object = None;
        let mut metadata_object = None;
        let mut regulated_metadata_object = None;
        for created in effects.created() {
            let object = test_cluster
                .get_object_from_fullnode_store(&created.object_id())
                .await
                .unwrap();
            if object.is_package() {
                continue;
            }
            let t = object.type_().unwrap();
            if t.is_coin() {
                assert!(new_coin_object.is_none());
                new_coin_object = Some(object);
            } else if t.is_coin_deny_cap() {
                assert!(deny_cap_object.is_none());
                deny_cap_object = Some(object);
            } else if t.is_regulated_coin_metadata() {
                assert!(regulated_metadata_object.is_none());
                regulated_metadata_object = Some(object);
            } else if t.is_coin_metadata() {
                assert!(metadata_object.is_none());
                metadata_object = Some(object);
            }
        }
        // Check that publishing the package minted the new coin, along with
        // the metadata, deny cap, and regulated metadata.
        // Check that all their fields are consistent.
        let new_coin_object = new_coin_object.unwrap();
        let metadata_object = metadata_object.unwrap();
        let deny_cap_object = deny_cap_object.unwrap();
        let deny_cap: CoinDenyCap = deny_cap_object.to_rust().unwrap();
        assert_eq!(deny_cap.id.id.bytes, deny_cap_object.id());

        let regulated_metadata_object = regulated_metadata_object.unwrap();
        let regulated_metadata: RegulatedCoinMetadata =
            regulated_metadata_object.to_rust().unwrap();
        assert_eq!(
            regulated_metadata.id.id.bytes,
            regulated_metadata_object.id()
        );
        assert_eq!(
            regulated_metadata.deny_cap_object.bytes,
            deny_cap_object.id()
        );
        assert_eq!(
            regulated_metadata.coin_metadata_object.bytes,
            metadata_object.id()
        );

        let mut test_context = Self {
            test_cluster,
            new_coin_id: new_coin_object.id(),
            new_coin_owner: new_coin_object.owner.get_address_owner_address().unwrap(),
            deny_cap_object_id: deny_cap_object.id(),
            deny_cap_object_owner: deny_cap_object.owner.get_address_owner_address().unwrap(),
        };
        // Transfer the new coin to a new address just so that it's different from the deny cap owner.
        // This helps make sure we don't have bugs like always denying the deny cap owner.
        let receiver = test_context.test_cluster.get_address_1();
        assert_ne!(test_context.new_coin_owner, receiver);
        test_context.transfer_new_coin(receiver).await.unwrap();

        test_context
    }

    async fn transfer_new_coin(
        &mut self,
        receiver: SuiAddress,
    ) -> anyhow::Result<SuiTransactionBlockResponse> {
        let tx_data = self
            .test_cluster
            .test_transaction_builder_with_sender(self.new_coin_owner)
            .await
            .transfer(
                self.test_cluster
                    .get_latest_object_ref(&self.new_coin_id)
                    .await,
                receiver,
            )
            .build();
        let tx = self.test_cluster.sign_transaction(&tx_data);
        let result = self
            .test_cluster
            .wallet
            .execute_transaction_may_fail(tx)
            .await;
        if result
            .as_ref()
            .is_ok_and(|r| r.effects.as_ref().unwrap().status().is_ok())
        {
            self.new_coin_owner = receiver;
        }
        result
    }

    async fn call_deny_list_api(&self, address: SuiAddress, action: DenyAction) {
        let function = match action {
            DenyAction::Deny => "deny_list_add",
            DenyAction::Undeny => "deny_list_remove",
        };
        let tx_data = self
            .test_cluster
            .test_transaction_builder_with_sender(self.deny_cap_object_owner)
            .await
            .move_call(
                SUI_FRAMEWORK_PACKAGE_ID,
                COIN_MODULE_NAME.as_str(),
                function,
                vec![
                    CallArg::Object(self.get_deny_list_object_arg().await),
                    CallArg::Object(ObjectArg::ImmOrOwnedObject(
                        self.test_cluster
                            .get_latest_object_ref(&self.deny_cap_object_id)
                            .await,
                    )),
                    CallArg::Pure(bcs::to_bytes(&address).unwrap()),
                ],
            )
            .with_type_args(vec![self.get_new_coin_type().await])
            .build();
        let effects = self
            .test_cluster
            .sign_and_execute_transaction(&tx_data)
            .await
            .effects
            .unwrap();
        debug!(
            "call_deny_list_api with action {:?} effects: {:?}",
            action, effects
        );
    }

    async fn get_deny_list_object_arg(&self) -> ObjectArg {
        let coin_deny_list_object_init_version = self
            .test_cluster
            .fullnode_handle
            .sui_node
            .state()
            .epoch_store_for_testing()
            .epoch_start_config()
            .coin_deny_list_obj_initial_shared_version()
            .unwrap();
        ObjectArg::SharedObject {
            id: SUI_DENY_LIST_OBJECT_ID,
            initial_shared_version: coin_deny_list_object_init_version,
            mutable: true,
        }
    }

    async fn get_new_coin_type(&self) -> TypeTag {
        self.test_cluster
            .get_object_from_fullnode_store(&self.new_coin_id)
            .await
            .unwrap()
            .coin_type_maybe()
            .unwrap()
    }

    async fn get_new_coin_package_id(&self) -> ObjectID {
        match self.get_new_coin_type().await {
            TypeTag::Struct(struct_tag) => ObjectID::from(struct_tag.address),
            _ => panic!("Expected Struct TypeTag"),
        }
    }
}
