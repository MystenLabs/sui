// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

use std::collections::HashSet;
use std::net::SocketAddr;
use std::path::PathBuf;
use sui_core::authority_client::AuthorityAPI;
use sui_macros::*;
use sui_test_transaction_builder::publish_package;
use sui_types::base_types::{ObjectID, ObjectRef};
use sui_types::effects::TransactionEffectsAPI;
use sui_types::effects::{TransactionEffects, TransactionEvents};
use sui_types::error::{SuiError, UserInputError};
use sui_types::object::Owner;
use sui_types::transaction::{CallArg, ObjectArg, Transaction};
use test_cluster::{TestCluster, TestClusterBuilder};

#[sim_test]
async fn receive_object_feature_deny() {
    use sui_protocol_config::ProtocolConfig;

    let _guard = ProtocolConfig::apply_overrides_for_testing(|_, mut config| {
        config.set_receive_object_for_testing(false);
        config
    });

    let env = TestEnvironment::new().await;
    let (parent, child) = env.start().await;
    let arguments = vec![
        CallArg::Object(ObjectArg::ImmOrOwnedObject(parent)),
        CallArg::Object(ObjectArg::Receiving(child)),
    ];
    let txn = env.create_move_call("receiver", arguments).await;
    let err = env
        .test_cluster
        .authority_aggregator()
        .authority_clients
        .values()
        .next()
        .unwrap()
        .authority_client()
        .handle_transaction(txn, Some(SocketAddr::new([127, 0, 0, 1].into(), 0)))
        .await
        .map(|_| ())
        .unwrap_err();

    assert!(matches!(
        err,
        SuiError::UserInputError {
            error: UserInputError::Unsupported(..)
        }
    ));
}

#[sim_test]
async fn receive_of_object() {
    let env = TestEnvironment::new().await;
    let (parent, child) = env.start().await;
    env.receive(parent, child).await.unwrap();
}

#[sim_test]
async fn receive_of_object_with_reconfiguration() {
    let env = TestEnvironment::new().await;
    let (parent, child) = env.start().await;
    env.receive(parent, child).await.unwrap();
    env.test_cluster.trigger_reconfiguration().await;
}

#[sim_test]
async fn receive_of_object_with_reconfiguration_receive_after_reconfig() {
    let env = TestEnvironment::new().await;
    let (parent, child) = env.start().await;
    let (new_parent, new_child) = env.receive(parent, child).await.unwrap();
    env.test_cluster.trigger_reconfiguration().await;
    assert!(env.receive(new_parent, new_child).await.is_ok());
}

#[sim_test]
async fn receive_of_object_with_reconfiguration_receive_of_old_child_after_reconfig() {
    let env = TestEnvironment::new().await;
    let (parent, child) = env.start().await;
    let (new_parent, _) = env.receive(parent, child).await.unwrap();
    env.test_cluster.trigger_reconfiguration().await;
    assert!(env.receive(new_parent, child).await.is_err());
}

#[sim_test]
async fn receive_of_object_with_reconfiguration_receive_of_old_parent_after_reconfig() {
    let env = TestEnvironment::new().await;
    let (parent, child) = env.start().await;
    let (_, new_child) = env.receive(parent, child).await.unwrap();
    env.test_cluster.trigger_reconfiguration().await;
    assert!(env.receive(parent, new_child).await.is_err());
}

#[sim_test]
async fn receive_of_object_with_reconfiguration_receive_of_old_parent_and_child_after_reconfig() {
    let env = TestEnvironment::new().await;
    let (parent, child) = env.start().await;
    env.receive(parent, child).await.unwrap();
    env.test_cluster.trigger_reconfiguration().await;
    assert!(env.receive(parent, child).await.is_err());
}

#[sim_test]
async fn receive_of_object_with_reconfiguration_receive_after_reconfig_with_invalid_child() {
    let env = TestEnvironment::new().await;
    let (parent, child) = env.start().await;
    let (new_parent, new_child) = env.receive(parent, child).await.unwrap();
    env.test_cluster.trigger_reconfiguration().await;
    assert!(env.receive(new_child, new_parent).await.is_err());
}

#[sim_test]
async fn delete_of_object_with_reconfiguration_receive_of_old_parent_and_child_after_reconfig() {
    let env = TestEnvironment::new().await;
    let (parent, child) = env.start().await;
    env.delete(parent, child).await;
    env.test_cluster.trigger_reconfiguration().await;
    assert!(env.receive(parent, child).await.is_err());
}

#[sim_test]
async fn delete_of_object_with_reconfiguration_receive_of_new_parent_and_old_child_after_reconfig()
{
    let env = TestEnvironment::new().await;
    let (parent, child) = env.start().await;
    let new_parent = env.delete(parent, child).await;
    env.test_cluster.trigger_reconfiguration().await;
    assert!(env.receive(new_parent, child).await.is_err());
}

fn get_parent_and_child(created: Vec<(ObjectRef, Owner)>) -> (ObjectRef, ObjectRef) {
    // make sure there is an object with an `AddressOwner` who matches the object ID of another
    // object.
    let created_addrs: HashSet<_> = created.iter().map(|((i, _, _), _)| i).collect();
    let (child, parent_id) = created
        .iter()
        .find_map(|child @ (_, owner)| match owner {
            Owner::AddressOwner(j) if created_addrs.contains(&ObjectID::from(*j)) => {
                Some((child, (*j).into()))
            }
            _ => None,
        })
        .unwrap();
    let parent = created
        .iter()
        .find(|((id, _, _), _)| *id == parent_id)
        .unwrap();
    (parent.0, child.0)
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
            .move_call(self.move_package, "tto", function, arguments)
            .build();
        self.test_cluster.wallet.sign_transaction(&transaction)
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

    async fn start(&self) -> (ObjectRef, ObjectRef) {
        let (fx, _) = self.move_call("start", vec![]).await.unwrap();
        assert!(fx.status().is_ok());

        get_parent_and_child(fx.created())
    }

    async fn receive(
        &self,
        parent: ObjectRef,
        child: ObjectRef,
    ) -> anyhow::Result<(ObjectRef, ObjectRef)> {
        let arguments = vec![
            CallArg::Object(ObjectArg::ImmOrOwnedObject(parent)),
            CallArg::Object(ObjectArg::Receiving(child)),
        ];
        let fx = self.move_call("receiver", arguments).await?;
        assert!(fx.0.status().is_ok());
        let new_child_ref =
            fx.0.mutated_excluding_gas()
                .iter()
                .find_map(
                    |(oref, _)| {
                        if oref.0 == child.0 {
                            Some(*oref)
                        } else {
                            None
                        }
                    },
                )
                .unwrap();
        let new_parent_ref =
            fx.0.mutated_excluding_gas()
                .iter()
                .find_map(|(oref, _)| {
                    if oref.0 == parent.0 {
                        Some(*oref)
                    } else {
                        None
                    }
                })
                .unwrap();
        Ok((new_parent_ref, new_child_ref))
    }

    async fn delete(&self, parent: ObjectRef, child: ObjectRef) -> ObjectRef {
        let arguments = vec![
            CallArg::Object(ObjectArg::ImmOrOwnedObject(parent)),
            CallArg::Object(ObjectArg::Receiving(child)),
        ];
        let fx = self.move_call("deleter", arguments).await.unwrap();
        assert!(fx.0.status().is_ok());
        fx.0.mutated_excluding_gas()
            .iter()
            .find_map(|(oref, _)| {
                if oref.0 == parent.0 {
                    Some(*oref)
                } else {
                    None
                }
            })
            .unwrap()
    }
}

async fn publish_move_package(test_cluster: &TestCluster) -> ObjectRef {
    let mut path = PathBuf::from(env!("CARGO_MANIFEST_DIR"));
    path.push("tests/move_test_code");
    publish_package(&test_cluster.wallet, path).await
}
