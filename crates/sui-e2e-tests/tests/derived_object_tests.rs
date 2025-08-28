// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

use std::path::PathBuf;
use sui_macros::*;
use sui_test_transaction_builder::publish_package;
use sui_types::base_types::{ObjectID, ObjectRef, SuiAddress};
use sui_types::derived_object::derive_object_id;
use sui_types::dynamic_field;
use sui_types::effects::TransactionEffectsAPI;
use sui_types::effects::{TransactionEffects, TransactionEvents};
use sui_types::object::Owner;
use sui_types::storage::WriteKind;
use sui_types::transaction::{CallArg, ObjectArg, Transaction};
use test_cluster::{TestCluster, TestClusterBuilder};

#[sim_test]
async fn derived_object_create_then_transfer_and_finally_receive() {
    let mut env = TestEnvironment::new().await;
    let parent = env.new_parent().await;

    let derived = env.new_derived(parent, 0u64, false).await;

    // Transfer the `any_obj` into the derived object's address
    let any_obj = env.new_any_obj(derived.0.into()).await;

    // Success -- we were able to "receive" an object transferred to our derived addr.
    let (_, owner) = env.receive(derived, any_obj).await;

    // The owner of the derived obj must now be the sender (since we received it and self-receive transfers to
    // ctx.sender()).
    assert_eq!(owner, Owner::AddressOwner(env.sender()))
}

#[sim_test]
async fn derived_object_claim_then_receive_already_transferred_object() {
    let env = TestEnvironment::new().await;
    let parent = env.new_parent().await;

    let derived_calculated_id = derive_object_id(
        parent.0,
        &sui_types::TypeTag::U64,
        &bcs::to_bytes(&0u64).unwrap(),
    )
    .unwrap();

    // Create a new object and transfer to our "derived" address before we have created
    // that derived address.
    let any_obj = env.new_any_obj(derived_calculated_id.into()).await;

    // If we are able to claim & receive, good :)
    let (fx, _) = env
        .move_call(
            "claim_and_receive",
            vec![
                CallArg::Object(ObjectArg::ImmOrOwnedObject(parent)),
                CallArg::Pure(0u64.to_le_bytes().to_vec()),
                CallArg::Object(ObjectArg::Receiving(any_obj)),
            ],
        )
        .await
        .unwrap();

    assert!(fx.status().is_ok());
    // We must have created 2 new objects DF (Claimed(DerivedID), DerivedObj)
    assert_eq!(fx.summary_for_debug().created_object_count, 2);
}

#[sim_test]
async fn derived_object_claim_and_add_df_in_one_tx() {
    let env = TestEnvironment::new().await;
    let parent = env.new_parent().await;

    let _derived = env.new_derived(parent, 0u64, true).await;
}

#[sim_test]
async fn derived_object_df_domain_separation() {
    let env = TestEnvironment::new().await;
    let parent = env.new_parent().await;

    let derived_calculated_id = derive_object_id(
        parent.0,
        &sui_types::TypeTag::U64,
        &bcs::to_bytes(&0u64).unwrap(),
    )
    .unwrap();

    let df_calculated_id = dynamic_field::derive_dynamic_field_id(
        parent.0,
        &sui_types::TypeTag::U64,
        &bcs::to_bytes(&0u64).unwrap(),
    )
    .unwrap();

    let (fx, _) = env
        .move_call(
            "df_domain_separation",
            vec![
                CallArg::Object(ObjectArg::ImmOrOwnedObject(parent)),
                CallArg::Pure(0u64.to_le_bytes().to_vec()),
            ],
        )
        .await
        .unwrap();

    assert!(fx.status().is_ok());

    // Verify that we have the DF and the derived object as expected, and they are not the same id.
    assert_ne!(derived_calculated_id, df_calculated_id);

    vec![derived_calculated_id, df_calculated_id]
        .iter()
        .for_each(|id| {
            let _ = fx
                .all_changed_objects()
                .iter()
                .find(|obj| obj.0 .0 == *id && obj.2 == WriteKind::Create)
                .cloned()
                .unwrap();
        });
}

#[sim_test]
/// Test deriving the UID, claiming an object that was transferred ahead of time,
/// and destroying the UID in the same transaction.
async fn derived_object_claim_and_receive_ephemeral() {
    let env = TestEnvironment::new().await;
    let parent = env.new_parent().await;

    let derived_calculated_id = derive_object_id(
        parent.0,
        &sui_types::TypeTag::U64,
        &bcs::to_bytes(&0u64).unwrap(),
    )
    .unwrap();

    let any_obj = env.new_any_obj(derived_calculated_id.into()).await;

    let (fx, _) = env
        .move_call(
            "claim_and_receive_ephemeral",
            vec![
                CallArg::Object(ObjectArg::ImmOrOwnedObject(parent)),
                CallArg::Pure(0u64.to_le_bytes().to_vec()),
                CallArg::Object(ObjectArg::Receiving(any_obj)),
            ],
        )
        .await
        .unwrap();

    assert!(fx.status().is_ok());
}

#[sim_test]
async fn derived_object_nested_claim_and_receive() {
    let env = TestEnvironment::new().await;
    let parent = env.new_parent().await;

    let derived_a = derive_object_id(
        parent.0,
        &sui_types::TypeTag::U64,
        &bcs::to_bytes(&0u64).unwrap(),
    )
    .unwrap();

    // Derive another object from the first derived object (parent -> derived_a -> derived_b)
    let derived_b = derive_object_id(
        derived_a,
        &sui_types::TypeTag::U64,
        &bcs::to_bytes(&0u64).unwrap(),
    )
    .unwrap();

    let any_obj = env.new_any_obj(derived_a.into()).await;
    let any_obj_b = env.new_any_obj(derived_b.into()).await;

    let (fx, _) = env
        .move_call(
            "nested_derived_claim_and_receive",
            vec![
                CallArg::Object(ObjectArg::ImmOrOwnedObject(parent)),
                CallArg::Pure(0u64.to_le_bytes().to_vec()),
                CallArg::Object(ObjectArg::Receiving(any_obj)),
                CallArg::Object(ObjectArg::Receiving(any_obj_b)),
            ],
        )
        .await
        .unwrap();

    assert!(fx.status().is_ok());
}

fn get_created_object(fx: &TransactionEffects, id: Option<ObjectID>) -> ObjectRef {
    let obj = fx
        .all_changed_objects()
        .iter()
        .find(|obj| {
            obj.2 == WriteKind::Create && (id.is_none() || id.is_some_and(|id| obj.0 .0 == id))
        })
        .unwrap()
        .clone();

    eprintln!("obj: {:?}", obj);

    obj.0
}

struct TestEnvironment {
    pub test_cluster: TestCluster,
    move_package: ObjectID,
}

impl TestEnvironment {
    async fn new() -> Self {
        let test_cluster = TestClusterBuilder::new().build().await;

        let move_package = publish_move_package(&test_cluster).await.0;

        Self {
            test_cluster,
            move_package,
        }
    }

    async fn create_move_call(
        &self,
        function: &'static str,
        arguments: Vec<CallArg>,
    ) -> Transaction {
        let transaction = self
            .test_cluster
            .test_transaction_builder()
            .await
            .move_call(self.move_package, "derived", function, arguments)
            .build();
        self.test_cluster
            .wallet
            .sign_transaction(&transaction)
            .await
    }

    async fn move_call(
        &self,
        function: &'static str,
        arguments: Vec<CallArg>,
    ) -> anyhow::Result<(TransactionEffects, TransactionEvents)> {
        let transaction = self.create_move_call(function, arguments).await;
        self.test_cluster
            .execute_transaction_return_raw_effects(transaction)
            .await
    }

    // Create a new `Parent` object
    async fn new_parent(&self) -> ObjectRef {
        let (fx, _) = self.move_call("create_parent", vec![]).await.unwrap();
        assert!(fx.status().is_ok());

        // Find the only created object that has to be the "parent" we created.
        get_created_object(&fx, None)
    }

    // Create a new `AnyObj` object (treated as a "random" object)
    async fn new_any_obj(&self, recipient: SuiAddress) -> ObjectRef {
        let arguments = vec![CallArg::Pure(recipient.to_vec())];

        let (fx, _) = self.move_call("create_any_obj", arguments).await.unwrap();
        assert!(fx.status().is_ok());

        // Find the only created object that has to be the "any_obj" we created.
        get_created_object(&fx, None)
    }

    // Create a new `Derived` object.
    // If `with_df` is true, the derived object will have a dynamic field added to it, for testing purposes
    // (mainly to test that the fresh object does not get into "modified" state.)
    async fn new_derived(&self, parent: ObjectRef, key: u64, with_df: bool) -> ObjectRef {
        let arguments = vec![
            CallArg::Object(ObjectArg::ImmOrOwnedObject(parent)),
            CallArg::Pure(key.to_le_bytes().to_vec()),
        ];
        let (fx, _) = self
            .move_call(
                if with_df {
                    "create_derived_with_df"
                } else {
                    "create_derived"
                },
                arguments,
            )
            .await
            .unwrap();
        assert!(fx.status().is_ok());

        let derived_id = derive_object_id(
            parent.0,
            &sui_types::TypeTag::U64,
            &bcs::to_bytes(&key).unwrap(),
        )
        .unwrap();

        get_created_object(&fx, Some(derived_id))
    }

    async fn receive(&self, derived: ObjectRef, child: ObjectRef) -> (ObjectID, Owner) {
        let arguments = vec![
            CallArg::Object(ObjectArg::ImmOrOwnedObject(derived)),
            CallArg::Object(ObjectArg::Receiving(child)),
        ];

        let (fx, _) = self.move_call("receive", arguments).await.unwrap();

        assert!(fx.status().is_ok());

        // Find the "child" object we received.
        let obj = fx
            .all_changed_objects()
            .iter()
            .find(|obj| obj.0 .0 == child.0)
            .cloned()
            .unwrap();

        (obj.0 .0, obj.1)
    }

    fn sender(&mut self) -> SuiAddress {
        self.test_cluster.wallet.active_address().unwrap()
    }
}

async fn publish_move_package(test_cluster: &TestCluster) -> ObjectRef {
    let mut path = PathBuf::from(env!("CARGO_MANIFEST_DIR"));
    path.push("tests/move_test_code");
    publish_package(&test_cluster.wallet, path).await
}
