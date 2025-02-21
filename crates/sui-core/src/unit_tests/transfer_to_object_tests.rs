// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

use std::{collections::HashSet, sync::Arc};

use sui_types::{
    base_types::{ObjectID, ObjectRef, SequenceNumber, SuiAddress},
    crypto::{get_key_pair, AccountKeyPair},
    digests::ObjectDigest,
    effects::{TransactionEffects, TransactionEffectsAPI},
    error::{SuiError, UserInputError},
    execution_status::{ExecutionFailureStatus, ExecutionStatus},
    object::{Object, Owner},
    programmable_transaction_builder::ProgrammableTransactionBuilder,
    transaction::{
        CallArg, ObjectArg, ProgrammableTransaction, VerifiedCertificate,
        TEST_ONLY_GAS_UNIT_FOR_PUBLISH,
    },
};

use crate::{
    authority::{
        authority_test_utils::{certify_transaction, send_consensus},
        authority_tests::{
            build_programmable_transaction, execute_programmable_transaction,
            execute_programmable_transaction_with_shared,
        },
        move_integration_tests::build_and_publish_test_package_with_upgrade_cap,
        test_authority_builder::TestAuthorityBuilder,
        AuthorityState,
    },
    move_call,
};
use move_core_types::ident_str;

// The primary use for these tests is to make sure the generated effect sets match what we expect
// when receiving an object, and if we then perform different types of operations on the received
// object (e.g., deleting, wrapping, unwrapping, adding as a dynamic field, etc.) and various
// combinations of that. Some of these tests also check and validate locking behavior around
// receiving object arguments as well.

// Run the test twice -- once with aggressive pruning enabled, and the other with it not enabled.
macro_rules! transfer_test_runner {
    (gas_objects: $num:expr, $expr:expr) => {
        let runner = TestRunner::new_with_objects("tto", $num, false).await;
        #[allow(clippy::redundant_closure_call)]
        $expr(runner).await;
        let runner = TestRunner::new_with_objects("tto", $num, true).await;
        #[allow(clippy::redundant_closure_call)]
        $expr(runner).await;
    };
    ($expr:expr) => {
        let runner = TestRunner::new("tto", false).await;
        #[allow(clippy::redundant_closure_call)]
        $expr(runner).await;
        let runner = TestRunner::new("tto", true).await;
        #[allow(clippy::redundant_closure_call)]
        $expr(runner).await;
    };
}

struct TestRunner {
    pub sender: SuiAddress,
    pub sender_key: AccountKeyPair,
    pub gas_object_ids: Vec<ObjectID>,
    pub authority_state: Arc<AuthorityState>,
    pub package: ObjectRef,
    pub upgrade_cap: ObjectRef,
    pub rgp: u64,
    pub aggressive_pruning_enabled: bool,
}

impl TestRunner {
    pub async fn new_with_objects(
        base_package_name: &str,
        num: usize,
        aggressive_pruning_enabled: bool,
    ) -> Self {
        telemetry_subscribers::init_for_testing();
        let (sender, sender_key): (_, AccountKeyPair) = get_key_pair();

        let authority_state = TestAuthorityBuilder::new().build().await;
        let rgp = authority_state.reference_gas_price_for_testing().unwrap();
        let mut gas_object_ids = vec![];
        for _ in 0..num {
            let gas_object_id = ObjectID::random();
            let gas_object = Object::with_id_owner_for_testing(gas_object_id, sender);
            authority_state.insert_genesis_object(gas_object).await;
            gas_object_ids.push(gas_object_id);
        }

        let (package, upgrade_cap) = build_and_publish_test_package_with_upgrade_cap(
            &authority_state,
            &sender,
            &sender_key,
            &gas_object_ids[0],
            base_package_name,
            /* with_unpublished_deps */ false,
        )
        .await;

        Self {
            sender,
            sender_key,
            gas_object_ids,
            authority_state,
            package,
            upgrade_cap,
            rgp,
            aggressive_pruning_enabled,
        }
    }

    pub async fn new(base_package_name: &str, aggressive_pruning_enabled: bool) -> Self {
        Self::new_with_objects(base_package_name, 1, aggressive_pruning_enabled).await
    }

    pub async fn signing_error(&mut self, pt: ProgrammableTransaction) -> SuiError {
        execute_programmable_transaction(
            &self.authority_state,
            &self.gas_object_ids[0],
            &self.sender,
            &self.sender_key,
            pt,
            self.rgp * TEST_ONLY_GAS_UNIT_FOR_PUBLISH,
        )
        .await
        .map(|_| ())
        .unwrap_err()
    }

    pub async fn run_with_gas_object(
        &mut self,
        pt: ProgrammableTransaction,
        idx: usize,
    ) -> TransactionEffects {
        let effects = execute_programmable_transaction(
            &self.authority_state,
            &self.gas_object_ids[idx],
            &self.sender,
            &self.sender_key,
            pt,
            self.rgp * TEST_ONLY_GAS_UNIT_FOR_PUBLISH,
        )
        .await
        .unwrap();

        if self.aggressive_pruning_enabled {
            self.authority_state
                .database_for_testing()
                .prune_objects_immediately_for_testing(vec![effects.clone()])
                .await
                .unwrap();
        }

        if let Some(updated_cap) = effects
            .mutated()
            .into_iter()
            .find_map(|(cap, _)| (cap.0 == self.upgrade_cap.0).then_some(cap))
        {
            self.upgrade_cap = updated_cap;
        }

        effects
    }

    pub async fn run_with_gas_object_shared(
        &mut self,
        pt: ProgrammableTransaction,
        idx: usize,
    ) -> TransactionEffects {
        let effects = execute_programmable_transaction_with_shared(
            &self.authority_state,
            &self.gas_object_ids[idx],
            &self.sender,
            &self.sender_key,
            pt,
            self.rgp * TEST_ONLY_GAS_UNIT_FOR_PUBLISH,
        )
        .await
        .unwrap();

        if self.aggressive_pruning_enabled {
            self.authority_state
                .database_for_testing()
                .prune_objects_immediately_for_testing(vec![effects.clone()])
                .await
                .unwrap();
        }

        if let Some(updated_cap) = effects
            .mutated()
            .into_iter()
            .find_map(|(cap, _)| (cap.0 == self.upgrade_cap.0).then_some(cap))
        {
            self.upgrade_cap = updated_cap;
        }

        effects
    }

    pub async fn run(&mut self, pt: ProgrammableTransaction) -> TransactionEffects {
        self.run_with_gas_object(pt, 0).await
    }

    pub async fn lock_and_verify_transaction(
        &mut self,
        pt: ProgrammableTransaction,
        account_id: usize,
    ) -> VerifiedCertificate {
        let transaction = build_programmable_transaction(
            &self.authority_state,
            &self.gas_object_ids[account_id],
            &self.sender,
            &self.sender_key,
            pt,
            TEST_ONLY_GAS_UNIT_FOR_PUBLISH,
        )
        .await
        .unwrap();
        certify_transaction(&self.authority_state, transaction)
            .await
            .unwrap()
    }

    pub async fn execute_certificate(
        &mut self,
        ct: VerifiedCertificate,
        shared: bool,
    ) -> TransactionEffects {
        let epoch_store = self.authority_state.load_epoch_store_one_call_per_task();
        if shared {
            send_consensus(&self.authority_state, &ct).await;
        }
        // Call `execute_certificate` instead of `execute_certificate_with_execution_error` to make sure we go through TM
        let effects = self
            .authority_state
            .execute_certificate(&ct, &epoch_store)
            .await
            .unwrap();

        if self.aggressive_pruning_enabled {
            self.authority_state
                .database_for_testing()
                .prune_objects_immediately_for_testing(vec![effects.clone()])
                .await
                .unwrap();
        }
        effects
    }
}

fn get_parent_and_child(
    created: Vec<(ObjectRef, Owner)>,
) -> ((ObjectRef, Owner), (ObjectRef, Owner)) {
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
        .cloned()
        .unwrap();
    (parent, child.clone())
}

#[tokio::test]
async fn test_tto_transfer() {
    transfer_test_runner! { |mut runner: TestRunner| async move {
        let effects = runner
            .run({
                let mut builder = ProgrammableTransactionBuilder::new();
                move_call! {
                    builder,
                    (runner.package.0)::M1::start()
                };
                builder.finish()
            })
            .await;

        let (parent, child) = get_parent_and_child(effects.created());
        let transfer_digest = effects.transaction_digest();

        // No receive the sent object
        let effects = runner
            .run({
                let mut builder = ProgrammableTransactionBuilder::new();
                let parent = builder.obj(ObjectArg::ImmOrOwnedObject(parent.0)).unwrap();
                let child = builder.obj(ObjectArg::Receiving(child.0)).unwrap();
                move_call! {
                    builder,
                    (runner.package.0)::M1::receiver(parent, child)
                };
                builder.finish()
            })
            .await;

        assert!(effects.status().is_ok());
        assert!(effects.created().is_empty());
        assert!(effects.unwrapped().is_empty());
        assert!(effects.deleted().is_empty());
        assert!(effects.unwrapped_then_deleted().is_empty());
        assert!(effects.wrapped().is_empty());
        assert!(effects.dependencies().contains(transfer_digest));

        for (obj_ref, owner) in effects.mutated().iter() {
            if obj_ref.0 == child.0 .0 {
                // Child should be sent to 0x0
                assert_eq!(owner, &Owner::AddressOwner(SuiAddress::ZERO));
                // It's version should be bumped as well
                assert!(obj_ref.1 > child.0 .1);
            }
            if obj_ref.0 == parent.0 .0 {
                // owner of the parent stays the same
                assert_eq!(owner, &parent.1);
                // parent version is also bumped
                assert!(obj_ref.1 > parent.0 .1);
            }
        }
    }
    }
}

#[tokio::test]
async fn test_tto_intersection_input_and_receiving_objects() {
    transfer_test_runner! { |mut runner: TestRunner| async move {
        let effects = runner
            .run({
                let mut builder = ProgrammableTransactionBuilder::new();
                move_call! {
                    builder,
                    (runner.package.0)::M1::start()
                };
                builder.finish()
            })
            .await;

        let (parent, child) = get_parent_and_child(effects.created());
        let parent_receiving_arg = CallArg::Object(ObjectArg::Receiving(parent.0));
        let child_receiving_arg = CallArg::Object(ObjectArg::Receiving(child.0));

        // Duplicate object reference between receiving and input object arguments.
        let SuiError::UserInputError { error } = runner
            .signing_error({
                let mut builder = ProgrammableTransactionBuilder::new();
                let parent = builder.obj(ObjectArg::ImmOrOwnedObject(parent.0)).unwrap();
                let child = builder.obj(ObjectArg::Receiving(child.0)).unwrap();
                move_call! {
                    builder,
                    (runner.package.0)::M1::receiver(parent, child)
                };
                let mut built = builder.finish();
                built.inputs.push(parent_receiving_arg);
                built
            })
            .await else {
                panic!("expected signing error");
            };
        assert!(matches!(error, UserInputError::DuplicateObjectRefInput));

        // Duplicate object reference in receiving object arguments.
        let SuiError::UserInputError { error } = runner
            .signing_error({
                let mut builder = ProgrammableTransactionBuilder::new();
                let parent = builder.obj(ObjectArg::ImmOrOwnedObject(parent.0)).unwrap();
                let child = builder.obj(ObjectArg::Receiving(child.0)).unwrap();
                move_call! {
                    builder,
                    (runner.package.0)::M1::receiver(parent, child)
                };
                let mut built = builder.finish();
                built.inputs.push(child_receiving_arg);
                built
            })
            .await else {
                panic!("expected signing error");
            };
        assert!(matches!(error, UserInputError::DuplicateObjectRefInput));
    }
    }
}

#[tokio::test]
async fn test_tto_invalid_receiving_arguments() {
    transfer_test_runner! { |mut runner: TestRunner| async move {
        let effects = runner
            .run({
                let mut builder = ProgrammableTransactionBuilder::new();
                move_call! {
                    builder,
                    (runner.package.0)::M1::start()
                };
                builder.finish()
            })
            .await;

        let (parent, child) = get_parent_and_child(effects.created());
        let package_object_ref = runner.package;
        let shared = effects
            .created()
            .iter()
            .find(|(_, owner)| matches!(owner, Owner::Shared { .. }))
            .cloned()
            .unwrap();
        let immutable = effects
            .created()
            .iter()
            .find(|(_, owner)| matches!(owner, Owner::Immutable))
            .cloned()
            .unwrap();
        let object_owned = effects
            .created()
            .iter()
            .find(|(_, owner)| matches!(owner, Owner::ObjectOwner(_)))
            .cloned()
            .unwrap();

        #[allow(clippy::type_complexity)]
        let mutations: Vec<(
            Box<dyn FnOnce(ObjectRef) -> ObjectRef>,
            Box<dyn FnOnce(UserInputError) -> bool>,
        )> = vec![
            (
                Box::new(|x: ObjectRef| (x.0, SequenceNumber::MAX, x.2)),
                Box::new(|err| matches!(err, UserInputError::InvalidSequenceNumber)),
            ),
            (
                Box::new(|x: ObjectRef| (ObjectID::ZERO, x.1, x.2)),
                Box::new(|err| matches!(err, UserInputError::ObjectNotFound { .. })),
            ),
            (
                Box::new(|x: ObjectRef| (x.0, x.1.next(), x.2)),
                Box::new(|err| {
                    matches!(
                        err,
                        UserInputError::ObjectVersionUnavailableForConsumption { .. }
                    )
                }),
            ),
            (
                Box::new(|x: ObjectRef| (x.0, x.1.one_before().unwrap(), x.2)),
                Box::new(|err| {
                    matches!(
                        err,
                        UserInputError::ObjectVersionUnavailableForConsumption { .. }
                    )
                }),
            ),
            (
                Box::new(|_: ObjectRef| package_object_ref),
                Box::new(|err| matches!(err, UserInputError::MovePackageAsObject { .. })),
            ),
            (
                Box::new(|x: ObjectRef| (x.0, x.1, ObjectDigest::random())),
                Box::new(|err| matches!(err, UserInputError::InvalidObjectDigest { .. })),
            ),
            (
                Box::new(|_: ObjectRef| shared.0),
                Box::new(|err| matches!(err, UserInputError::NotSharedObjectError)),
            ),
            (
                Box::new(|_: ObjectRef| object_owned.0),
                Box::new(|err| matches!(err, UserInputError::InvalidChildObjectArgument { .. })),
            ),
            (
                Box::new(|_: ObjectRef| immutable.0),
                Box::new(|err| matches!(err, UserInputError::MutableParameterExpected { .. })),
            ),
        ];

        for (i, (mutate, expect)) in mutations.into_iter().enumerate() {
            let SuiError::UserInputError { error } = runner.signing_error({
                let mut builder = ProgrammableTransactionBuilder::new();
                let parent = builder.obj(ObjectArg::ImmOrOwnedObject(parent.0)).unwrap();
                let child = builder.obj(ObjectArg::Receiving(mutate(child.0))).unwrap();
                move_call! {
                    builder,
                    (runner.package.0)::M1::receiver(parent, child)
                };
                builder.finish()
            })
            .await else {
                panic!("failed on iteration {}", i);
            };
            assert!(
                expect(error),
                "failed to match expected error on iteration {}",
                i
            );
        }
    }
    }
}

#[tokio::test]
async fn test_tto_unused_receiver() {
    transfer_test_runner! { |mut runner: TestRunner| async move {
        let effects = runner
            .run({
                let mut builder = ProgrammableTransactionBuilder::new();
                move_call! {
                    builder,
                    (runner.package.0)::M1::start()
                };
                builder.finish()
            })
            .await;

        let (parent, child) = get_parent_and_child(effects.created());

        let effects = runner
            .run({
                let mut builder = ProgrammableTransactionBuilder::new();
                builder.obj(ObjectArg::ImmOrOwnedObject(parent.0)).unwrap();
                builder.obj(ObjectArg::Receiving(child.0)).unwrap();
                builder.finish()
            })
            .await;

        assert!(effects.status().is_ok());
        assert!(effects.created().is_empty());
        assert!(effects.unwrapped().is_empty());
        assert!(effects.deleted().is_empty());
        assert!(effects.unwrapped_then_deleted().is_empty());
        assert!(effects.wrapped().is_empty());

        // If the receiving argument is not used it should not be modified!
        assert!(!effects
            .modified_at_versions()
            .iter()
            .any(|(i, _)| i == &child.0 .0));
        // Since the parent was not used but it was an input object, it should be modified
        assert!(effects
            .modified_at_versions()
            .iter()
            .any(|(i, _)| i == &parent.0 .0));

        // Make sure parent exists in mutated, and the version is bumped.
        for (obj_ref, owner) in effects.mutated().iter() {
            if obj_ref.0 == parent.0 .0 {
                // owner of the parent stays the same
                assert_eq!(owner, &parent.1);
                // parent version is also bumped
                assert!(obj_ref.1 > parent.0 .1);
            }
        }
    }
    }
}

#[tokio::test]
async fn test_tto_pass_receiving_by_refs() {
    transfer_test_runner! { |mut runner: TestRunner| async move {
        let effects = runner
            .run({
                let mut builder = ProgrammableTransactionBuilder::new();
                move_call! {
                    builder,
                    (runner.package.0)::M1::start()
                };
                builder.finish()
            })
            .await;

        let (parent, child) = get_parent_and_child(effects.created());

        let effects = runner
            .run({
                let mut builder = ProgrammableTransactionBuilder::new();
                let parent = builder.obj(ObjectArg::ImmOrOwnedObject(parent.0)).unwrap();
                let child = builder.obj(ObjectArg::Receiving(child.0)).unwrap();
                move_call! {
                    builder,
                    (runner.package.0)::M1::call_immut_ref(parent, child)
                };
                move_call! {
                    builder,
                    (runner.package.0)::M1::call_mut_ref(parent, child)
                };
                builder.finish()
            })
            .await;

        assert!(effects.status().is_ok());
        assert!(effects.created().is_empty());
        assert!(effects.unwrapped().is_empty());
        assert!(effects.deleted().is_empty());
        assert!(effects.unwrapped_then_deleted().is_empty());
        assert!(effects.wrapped().is_empty());

        // If the receiving argument is not used it should not be modified!
        assert!(!effects
            .modified_at_versions()
            .iter()
            .any(|(i, _)| i == &child.0 .0));
        // Since the parent was not used but it was an input object, it should be modified
        assert!(effects
            .modified_at_versions()
            .iter()
            .any(|(i, _)| i == &parent.0 .0));

        // Make sure parent exists in mutated, and the version is bumped.
        for (obj_ref, owner) in effects.mutated().iter() {
            if obj_ref.0 == parent.0 .0 {
                // owner of the parent stays the same
                assert_eq!(owner, &parent.1);
                // parent version is also bumped
                assert!(obj_ref.1 > parent.0 .1);
            }
        }
    }
    }
}

#[tokio::test]
async fn test_tto_delete() {
    transfer_test_runner! { |mut runner: TestRunner| async move {
        let effects = runner
            .run({
                let mut builder = ProgrammableTransactionBuilder::new();
                move_call! {
                    builder,
                    (runner.package.0)::M1::start()
                };
                builder.finish()
            })
            .await;

        let (parent, child) = get_parent_and_child(effects.created());

        let effects = runner
            .run({
                let mut builder = ProgrammableTransactionBuilder::new();
                let parent = builder.obj(ObjectArg::ImmOrOwnedObject(parent.0)).unwrap();
                let child = builder.obj(ObjectArg::Receiving(child.0)).unwrap();
                move_call! {
                    builder,
                    (runner.package.0)::M1::deleter(parent, child)
                };
                builder.finish()
            })
            .await;

        assert!(effects.status().is_ok());
        assert!(effects.created().is_empty());
        assert!(effects.unwrapped().is_empty());
        assert!(effects.unwrapped_then_deleted().is_empty());
        assert!(effects.wrapped().is_empty());
        // Deleted should be non-empty
        assert_eq!(effects.deleted().len(), 1);
        // Deleted should contain the child object
        assert_eq!(effects.deleted()[0].0, child.0 .0);

        // Make sure parent exists in mutated, and the version is bumped.
        for (obj_ref, owner) in effects.mutated().iter() {
            if obj_ref.0 == parent.0 .0 {
                // owner of the parent stays the same
                assert_eq!(owner, &parent.1);
                // parent version is also bumped
                assert!(obj_ref.1 > parent.0 .1);
            }
        }
    }
    }
}

#[tokio::test]
async fn test_tto_wrap() {
    transfer_test_runner! { |mut runner: TestRunner| async move {
        let effects = runner
            .run({
                let mut builder = ProgrammableTransactionBuilder::new();
                move_call! {
                    builder,
                    (runner.package.0)::M1::start()
                };
                builder.finish()
            })
            .await;

        let (parent, child) = get_parent_and_child(effects.created());

        let effects = runner
            .run({
                let mut builder = ProgrammableTransactionBuilder::new();
                let parent = builder.obj(ObjectArg::ImmOrOwnedObject(parent.0)).unwrap();
                let child = builder.obj(ObjectArg::Receiving(child.0)).unwrap();
                move_call! {
                    builder,
                    (runner.package.0)::M1::wrapper(parent, child)
                };
                builder.finish()
            })
            .await;

        assert!(effects.status().is_ok());
        assert!(effects.unwrapped().is_empty());
        assert!(effects.unwrapped_then_deleted().is_empty());
        assert!(effects.deleted().is_empty());
        // We created an object since we wrapped this when we received the transaction
        assert_eq!(effects.created().len(), 1);
        // Wrapped should be non-empty
        assert_eq!(effects.wrapped().len(), 1);
        // Wrapped should contain the child object
        assert_eq!(effects.wrapped()[0].0, child.0 .0);

        // Make sure parent exists in mutated, and the version is bumped.
        for (obj_ref, owner) in effects.mutated().iter() {
            if obj_ref.0 == parent.0 .0 {
                // owner of the parent stays the same
                assert_eq!(owner, &parent.1);
                // parent version is also bumped
                assert!(obj_ref.1 > parent.0 .1);
            }
        }
    }
    }
}

#[tokio::test]
async fn test_tto_unwrap_transfer() {
    transfer_test_runner! { |mut runner: TestRunner| async move {
        let effects = runner
            .run({
                let mut builder = ProgrammableTransactionBuilder::new();
                move_call! {
                    builder,
                    (runner.package.0)::M2::start()
                };
                builder.finish()
            })
            .await;

        let (parent, child) = get_parent_and_child(effects.created());

        // No receive the sent object
        let effects = runner
            .run({
                let mut builder = ProgrammableTransactionBuilder::new();
                let parent = builder.obj(ObjectArg::ImmOrOwnedObject(parent.0)).unwrap();
                let child = builder.obj(ObjectArg::Receiving(child.0)).unwrap();
                move_call! {
                    builder,
                    (runner.package.0)::M2::unwrap_receiver(parent, child)
                };
                builder.finish()
            })
            .await;

        assert!(effects.status().is_ok());
        assert!(effects.created().is_empty());
        assert!(effects.unwrapped_then_deleted().is_empty());
        assert!(effects.wrapped().is_empty());

        // Unwrapped should be size 1
        assert_eq!(effects.unwrapped().len(), 1);
        // The now-unwrapped object should be sent to 0x0
        assert_eq!(
            effects.unwrapped()[0].1,
            Owner::AddressOwner(SuiAddress::ZERO)
        );

        // Receiving object ID is deleted
        assert_eq!(effects.deleted().len(), 1);
        // Deleted should contain the child object id
        assert_eq!(effects.deleted()[0].0, child.0 .0);

        for (obj_ref, owner) in effects.mutated().iter() {
            // child ref should not be mutated since it was deleted
            assert_ne!(obj_ref.0, child.0 .0);
            if obj_ref.0 == parent.0 .0 {
                // owner of the parent stays the same
                assert_eq!(owner, &parent.1);
                // parent version is also bumped
                assert!(obj_ref.1 > parent.0 .1);
            }
        }
    }
    }
}

#[tokio::test]
async fn test_tto_unwrap_delete() {
    transfer_test_runner! { |mut runner: TestRunner| async move {
        let effects = runner
            .run({
                let mut builder = ProgrammableTransactionBuilder::new();
                move_call! {
                    builder,
                    (runner.package.0)::M2::start()
                };
                builder.finish()
            })
            .await;

        let (parent, child) = get_parent_and_child(effects.created());

        // No receive the sent object
        let effects = runner
            .run({
                let mut builder = ProgrammableTransactionBuilder::new();
                let parent = builder.obj(ObjectArg::ImmOrOwnedObject(parent.0)).unwrap();
                let child = builder.obj(ObjectArg::Receiving(child.0)).unwrap();
                move_call! {
                    builder,
                    (runner.package.0)::M2::unwrap_deleter(parent, child)
                };
                builder.finish()
            })
            .await;

        assert!(effects.status().is_ok());
        assert!(effects.created().is_empty());
        assert!(effects.unwrapped().is_empty());
        assert!(effects.wrapped().is_empty());

        // The deleted should be of size 1, and should contain the child address
        assert_eq!(effects.deleted().len(), 1);
        assert_eq!(effects.deleted()[0].0, child.0 .0);

        // Unwrapped then deleted should be of size 1 since we deleted the inner object as well.
        assert_eq!(effects.unwrapped_then_deleted().len(), 1);

        for (obj_ref, owner) in effects.mutated().iter() {
            // child ref should not be mutated since it was deleted
            assert_ne!(obj_ref.0, child.0 .0);
            if obj_ref.0 == parent.0 .0 {
                // owner of the parent stays the same
                assert_eq!(owner, &parent.1);
                // parent version is also bumped
                assert!(obj_ref.1 > parent.0 .1);
            }
        }
    }
    }
}

#[tokio::test]
async fn test_tto_unwrap_add_as_dynamic_field() {
    transfer_test_runner! { |mut runner: TestRunner| async move {
        let effects = runner
            .run({
                let mut builder = ProgrammableTransactionBuilder::new();
                move_call! {
                    builder,
                    (runner.package.0)::M2::start()
                };
                builder.finish()
            })
            .await;

        let (parent, child) = get_parent_and_child(effects.created());

        // No receive the sent object
        let effects = runner
            .run({
                let mut builder = ProgrammableTransactionBuilder::new();
                let parent = builder.obj(ObjectArg::ImmOrOwnedObject(parent.0)).unwrap();
                let child = builder.obj(ObjectArg::Receiving(child.0)).unwrap();
                move_call! {
                    builder,
                    (runner.package.0)::M2::unwrap_add_dyn(parent, child)
                };
                builder.finish()
            })
            .await;

        assert!(effects.status().is_ok());
        // Since it's placed as a dynamic field it will be rewrapped(). So `unwrapped` should be empty
        assert!(effects.unwrapped().is_empty());
        // Similarly it was already wrapped, so even though we're wrapping with the dynamic field `wrapped` should be empty
        assert!(effects.wrapped().is_empty());
        assert!(effects.unwrapped_then_deleted().is_empty());

        assert_eq!(effects.created().len(), 1);

        // The deleted should be of size 1, and should contain the child address
        assert_eq!(effects.deleted().len(), 1);
        assert_eq!(effects.deleted()[0].0, child.0 .0);

        for (obj_ref, owner) in effects.mutated().iter() {
            assert_ne!(obj_ref.0, child.0 .0);
            if obj_ref.0 == parent.0 .0 {
                // owner of the parent stays the same
                assert_eq!(owner, &parent.1);
                // parent version is also bumped
                assert!(obj_ref.1 > parent.0 .1);
            }
        }
    }
    }
}

// This tests that locks are not grabbed for receiving objects.
// This test does this by
// 1. Creating a parent object and child object
// 2. Creating a fake parent object
// 3. Create and sign a transaction `tx1` that tries to receive the child object using
//    the fake parent.
// 4. Create and sign a transaction `tx2` that receives the child object using the valid parent
//    object.
// 5. Execute `tx2` and verify that it can be executed successfully.
// 6. Execute `tx1` and verify that it can be executed, but will result in a Move abort.
// The order of steps 5 and 6 are swapped if `flipper` is `true`.
// The object is deleted instead of received if `should_delete` is `true`.
async fn verify_tto_not_locked(
    flipper: bool,
    should_delete: bool,
    aggressive_pruning: bool,
) -> (TransactionEffects, TransactionEffects) {
    let mut runner = TestRunner::new_with_objects("tto", 2, aggressive_pruning).await;
    let effects = runner
        .run({
            let mut builder = ProgrammableTransactionBuilder::new();
            move_call! {
                builder,
                (runner.package.0)::M3::start()
            };
            builder.finish()
        })
        .await;

    let (parent, child) = get_parent_and_child(effects.created());
    let fake_parent = effects
        .created()
        .iter()
        .find(|(obj_ref, _)| obj_ref.0 != parent.0 .0 && obj_ref.0 != child.0 .0)
        .cloned()
        .unwrap();

    // Now get a certificate for fake_parent/child1. This will lock input objects.
    // NB: the receiving object is _not_ locked.
    let cert_for_fake_parent = runner
        .lock_and_verify_transaction(
            {
                let mut builder = ProgrammableTransactionBuilder::new();
                let parent = builder
                    .obj(ObjectArg::ImmOrOwnedObject(fake_parent.0))
                    .unwrap();
                let child = builder.obj(ObjectArg::Receiving(child.0)).unwrap();
                if should_delete {
                    move_call!(builder, (runner.package.0)::M3::deleter(parent, child));
                } else {
                    move_call!(builder, (runner.package.0)::M3::receiver(parent, child));
                };
                builder.finish()
            },
            0,
        )
        .await;

    // After the other (fake) transaction has been created and signed, sign and execute this
    // transaction. This should have no issues because the receiving object is not locked by the
    // signing of the transaction above.
    let valid_cert = runner
        .lock_and_verify_transaction(
            {
                let mut builder = ProgrammableTransactionBuilder::new();
                let parent = builder.obj(ObjectArg::ImmOrOwnedObject(parent.0)).unwrap();
                let child = builder.obj(ObjectArg::Receiving(child.0)).unwrap();
                if should_delete {
                    move_call!(builder, (runner.package.0)::M3::deleter(parent, child));
                } else {
                    move_call!(builder, (runner.package.0)::M3::receiver(parent, child));
                };
                builder.finish()
            },
            1,
        )
        .await;

    // The order of the execution of these transactions is flipped depending on the value of
    // flipper. However, the result should be the same in either case.
    let (valid_effects, invalid_effects) = if flipper {
        let invalid_effects = runner
            .execute_certificate(cert_for_fake_parent, false)
            .await;
        let valid_effects = runner.execute_certificate(valid_cert, false).await;
        (valid_effects, invalid_effects)
    } else {
        let valid_effects = runner.execute_certificate(valid_cert, false).await;
        let invalid_effects = runner
            .execute_certificate(cert_for_fake_parent, false)
            .await;
        (valid_effects, invalid_effects)
    };

    assert!(valid_effects.status().is_ok());
    assert!(invalid_effects.status().is_err());
    assert!(matches!(
        invalid_effects.status(),
        ExecutionStatus::Failure {
            error: ExecutionFailureStatus::MoveAbort(_, _),
            ..
        }
    ));
    (valid_effects, invalid_effects)
}

fn assert_effects_equivalent(ef1: &TransactionEffects, ef2: &TransactionEffects) {
    assert_eq!(ef1.status(), ef2.status());
    assert_eq!(ef1.executed_epoch(), ef2.executed_epoch());
    assert_eq!(ef1.gas_cost_summary(), ef2.gas_cost_summary());
    assert_eq!(
        ef1.modified_at_versions().len(),
        ef2.modified_at_versions().len()
    );
    assert_eq!(ef1.created().len(), ef2.created().len());
    assert_eq!(ef1.mutated().len(), ef2.mutated().len());
    assert_eq!(ef1.unwrapped().len(), ef2.unwrapped().len());
    assert_eq!(ef1.deleted().len(), ef2.deleted().len());
    assert_eq!(
        ef1.unwrapped_then_deleted().len(),
        ef2.unwrapped_then_deleted().len()
    );
    assert_eq!(ef1.wrapped().len(), ef2.wrapped().len());
    assert_eq!(ef1.dependencies().len(), ef2.dependencies().len());
}

#[tokio::test]
async fn test_tto_not_locked() {
    for aggressive_pruning_enabled in [true, false] {
        // The transaction effects for the valid and invalid transactions should be the same regardless
        // of the order in which they are run.
        let (valid1, invalid1) =
            verify_tto_not_locked(false, false, aggressive_pruning_enabled).await;
        let (valid2, invalid2) =
            verify_tto_not_locked(true, false, aggressive_pruning_enabled).await;
        assert_effects_equivalent(&valid1, &valid2);
        assert_effects_equivalent(&invalid1, &invalid2);

        // The same should hold if the object is deleted by an intervening transaction.
        let (valid1, invalid1) =
            verify_tto_not_locked(false, true, aggressive_pruning_enabled).await;
        let (valid2, invalid2) =
            verify_tto_not_locked(true, true, aggressive_pruning_enabled).await;
        assert_effects_equivalent(&valid1, &valid2);
        assert_effects_equivalent(&invalid1, &invalid2);
    }
}

#[tokio::test]
async fn test_tto_valid_dependencies() {
    transfer_test_runner! {gas_objects: 3, |mut runner: TestRunner| async move {
        let effects = runner
            .run({
                let mut builder = ProgrammableTransactionBuilder::new();
                move_call! {
                    builder,
                    (runner.package.0)::M4::start1()
                };
                builder.finish()
            })
            .await;
        let parent = effects.created()[0].clone();

        let effects = runner
            .run({
                let mut builder = ProgrammableTransactionBuilder::new();
                move_call! {
                    builder,
                    (runner.package.0)::M4::start2()
                };
                builder.finish()
            })
            .await;
        let child = effects.created()[0].clone();

        // Use a different gas coin than for all the other transactions. This serves two purposes:
        // 1. Makes sure that we are registering the dependency on the transaction that transferred the
        //    object solely because of the fact that we received it in this transaction.
        // 2. Since the gas coin is fresh it will have a smaller version, so this will test that we
        //    properly compute and update the lamport version that we should use for the transaction.
        let effects = runner
            .run_with_gas_object(
                {
                    let mut builder = ProgrammableTransactionBuilder::new();
                    builder
                        .transfer_object(SuiAddress::from(parent.0 .0), child.0)
                        .unwrap();
                    builder.finish()
                },
                1,
            )
            .await;

        let child = effects
            .mutated()
            .iter()
            .find(|(o, _)| o.0 == child.0 .0)
            .cloned()
            .unwrap();
        let transfer_digest = effects.transaction_digest();

        // No receive the sent object
        let effects = runner
            .run_with_gas_object(
                {
                    let mut builder = ProgrammableTransactionBuilder::new();
                    let parent = builder.obj(ObjectArg::ImmOrOwnedObject(parent.0)).unwrap();
                    let child = builder.obj(ObjectArg::Receiving(child.0)).unwrap();
                    move_call! {
                        builder,
                        (runner.package.0)::M4::receiver(parent, child)
                    };
                    builder.finish()
                },
                2,
            )
            .await;

        assert!(effects.status().is_ok());
        assert!(effects.created().is_empty());
        assert!(effects.unwrapped().is_empty());
        assert!(effects.deleted().is_empty());
        assert!(effects.unwrapped_then_deleted().is_empty());
        assert!(effects.wrapped().is_empty());
        assert!(effects.dependencies().contains(transfer_digest));

        for (obj_ref, owner) in effects.mutated().iter() {
            if obj_ref.0 == child.0 .0 {
                // Child should be sent to 0x0
                assert_eq!(owner, &Owner::AddressOwner(SuiAddress::ZERO));
                // It's version should be bumped as well
                assert!(obj_ref.1 > child.0 .1);
                // The child should be the max version
                assert_eq!(obj_ref.1.value(), child.0 .1.value() + 1);
            }
            if obj_ref.0 == parent.0 .0 {
                // owner of the parent stays the same
                assert_eq!(owner, &parent.1);
                // parent version is also bumped
                assert!(obj_ref.1 > parent.0 .1);
                // The child should be the max version
                assert_eq!(obj_ref.1.value(), child.0 .1.value() + 1);
            }
        }
    }
    }
}

#[tokio::test]
async fn test_tto_valid_dependencies_delete_on_receive() {
    transfer_test_runner! {gas_objects: 3, |mut runner: TestRunner| async move {
        let effects = runner
            .run({
                let mut builder = ProgrammableTransactionBuilder::new();
                move_call! {
                    builder,
                    (runner.package.0)::M4::start1()
                };
                builder.finish()
            })
            .await;
        let parent = effects.created()[0].clone();

        let effects = runner
            .run({
                let mut builder = ProgrammableTransactionBuilder::new();
                move_call! {
                    builder,
                    (runner.package.0)::M4::start2()
                };
                builder.finish()
            })
            .await;
        let child = effects.created()[0].clone();

        // Use a different gas coin than for all the other transactions. This serves two purposes:
        // 1. Makes sure that we are registering the dependency on the transaction that transferred the
        //    object solely because of the fact that we received it in this transaction.
        // 2. Since the gas coin is fresh it will have a smaller version, so this will test that we
        //    properly compute and update the lamport version that we should use for the transaction.
        let effects = runner
            .run_with_gas_object(
                {
                    let mut builder = ProgrammableTransactionBuilder::new();
                    builder
                        .transfer_object(SuiAddress::from(parent.0 .0), child.0)
                        .unwrap();
                    builder.finish()
                },
                1,
            )
            .await;

        let child = effects
            .mutated()
            .iter()
            .find(|(o, _)| o.0 == child.0 .0)
            .cloned()
            .unwrap();
        let transfer_digest = effects.transaction_digest();

        // No receive and delete the sent object
        let effects = runner
            .run_with_gas_object(
                {
                    let mut builder = ProgrammableTransactionBuilder::new();
                    let parent = builder.obj(ObjectArg::ImmOrOwnedObject(parent.0)).unwrap();
                    let child = builder.obj(ObjectArg::Receiving(child.0)).unwrap();
                    move_call! {
                        builder,
                        (runner.package.0)::M4::deleter(parent, child)
                    };
                    builder.finish()
                },
                2,
            )
            .await;

        assert!(effects.status().is_ok());
        assert!(effects.created().is_empty());
        assert!(effects.unwrapped().is_empty());
        assert!(effects.unwrapped_then_deleted().is_empty());
        assert!(effects.wrapped().is_empty());
        // Deleted should be non-empty
        assert_eq!(effects.deleted().len(), 1);
        // Deleted should contain the child object
        assert_eq!(effects.deleted()[0].0, child.0 .0);
        assert!(effects.dependencies().contains(transfer_digest));

        // Make sure parent exists in mutated, and the version is bumped and is equal to the child's
        // version + 1 since the child has the highest version number in the transaction.
        for (obj_ref, owner) in effects.mutated().iter() {
            if obj_ref.0 == parent.0 .0 {
                // owner of the parent stays the same
                assert_eq!(owner, &parent.1);
                // parent version is also bumped
                assert!(obj_ref.1 > parent.0 .1);
                assert_eq!(obj_ref.1.value(), child.0 .1.value() + 1);
            }
        }
    }
    }
}

#[tokio::test]
async fn test_tto_dependencies_dont_receive() {
    transfer_test_runner! {gas_objects: 3, |mut runner: TestRunner| async move {
        let effects = runner
            .run({
                let mut builder = ProgrammableTransactionBuilder::new();
                move_call! {
                    builder,
                    (runner.package.0)::M4::start1()
                };
                builder.finish()
            })
            .await;
        let parent = effects.created()[0].clone();

        let effects = runner
            .run({
                let mut builder = ProgrammableTransactionBuilder::new();
                move_call! {
                    builder,
                    (runner.package.0)::M4::start2()
                };
                builder.finish()
            })
            .await;
        let old_child = effects.created()[0].clone();

        // Use a different gas coin than for all the other transactions. This:
        // 1. Makes sure that we are registering the dependency on the transaction that transferred the
        //    object solely because of the fact that we received it in this transaction.
        // 2. Since the gas coin is fresh it will have a smaller version, so this will test that we
        //    properly compute and update the lamport version that we should use for the transaction.
        let effects = runner
            .run_with_gas_object(
                {
                    let mut builder = ProgrammableTransactionBuilder::new();
                    builder
                        .transfer_object(SuiAddress::from(parent.0 .0), old_child.0)
                        .unwrap();
                    builder.finish()
                },
                1,
            )
            .await;

        let child = effects
            .mutated()
            .iter()
            .find(|(o, _)| o.0 == old_child.0 .0)
            .cloned()
            .unwrap();
        let transfer_digest = effects.transaction_digest();

        // ensure child version is greater than parent version, otherwise the check afterwards won't be
        // checking the correct thing.
        assert!(parent.0 .1.value() < child.0 .1.value());

        // Now dont receive the sent object but include it in the arguments for the PTB.
        let effects = runner
            .run_with_gas_object(
                {
                    let mut builder = ProgrammableTransactionBuilder::new();
                    let parent = builder.obj(ObjectArg::ImmOrOwnedObject(parent.0)).unwrap();
                    let child = builder.obj(ObjectArg::Receiving(child.0)).unwrap();
                    move_call! {
                        builder,
                        (runner.package.0)::M4::nop(parent, child)
                    };
                    builder.finish()
                },
                2,
            )
            .await;

        assert!(effects.status().is_ok());
        assert!(effects.created().is_empty());
        assert!(effects.unwrapped().is_empty());
        assert!(effects.deleted().is_empty());
        assert!(effects.unwrapped_then_deleted().is_empty());
        assert!(effects.wrapped().is_empty());
        // Not received so dependency is not added.
        assert!(!effects.dependencies().contains(transfer_digest));

        for (obj_ref, owner) in effects.mutated().iter() {
            assert_ne!(obj_ref.0, child.0 .0);
            if obj_ref.0 == parent.0 .0 {
                // owner of the parent stays the same
                assert_eq!(owner, &parent.1);
                // parent version is also bumped
                assert!(obj_ref.1 > parent.0 .1);
                // Parent version is the largest in this transaction
                assert_eq!(obj_ref.1.value(), child.0 .1.value() + 1);
            }
        }
    }
    }
}

#[tokio::test]
async fn test_tto_dependencies_dont_receive_but_abort() {
    transfer_test_runner! {gas_objects: 3, |mut runner: TestRunner| async move {
        let effects = runner
            .run({
                let mut builder = ProgrammableTransactionBuilder::new();
                move_call! {
                    builder,
                    (runner.package.0)::M4::start1()
                };
                builder.finish()
            })
            .await;
        let parent = effects.created()[0].clone();

        let effects = runner
            .run({
                let mut builder = ProgrammableTransactionBuilder::new();
                move_call! {
                    builder,
                    (runner.package.0)::M4::start2()
                };
                builder.finish()
            })
            .await;
        let old_child = effects.created()[0].clone();

        // Use a different gas coin than for all the other transactions. This:
        // 1. Makes sure that we are registering the dependency on the transaction that transferred the
        //    object solely because of the fact that we received it in this transaction.
        // 2. Since the gas coin is fresh it will have a smaller version, so this will test that we
        //    properly compute and update the lamport version that we should use for the transaction.
        let effects = runner
            .run_with_gas_object(
                {
                    let mut builder = ProgrammableTransactionBuilder::new();
                    builder
                        .transfer_object(SuiAddress::from(parent.0 .0), old_child.0)
                        .unwrap();
                    builder.finish()
                },
                1,
            )
            .await;

        let child = effects
            .mutated()
            .iter()
            .find(|(o, _)| o.0 == old_child.0 .0)
            .cloned()
            .unwrap();
        let transfer_digest = effects.transaction_digest();

        assert!(parent.0 .1.value() < child.0 .1.value());

        let effects = runner
            .run_with_gas_object(
                {
                    let mut builder = ProgrammableTransactionBuilder::new();
                    let parent = builder.obj(ObjectArg::ImmOrOwnedObject(parent.0)).unwrap();
                    let child = builder.obj(ObjectArg::Receiving(child.0)).unwrap();
                    move_call! {
                        builder,
                        (runner.package.0)::M4::aborter(parent, child)
                    };
                    builder.finish()
                },
                2,
            )
            .await;

        assert!(effects.status().is_err());
        assert!(effects.created().is_empty());
        assert!(effects.unwrapped().is_empty());
        assert!(effects.deleted().is_empty());
        assert!(effects.unwrapped_then_deleted().is_empty());
        assert!(effects.wrapped().is_empty());
        // Not received so dependency is not added.
        assert!(!effects.dependencies().contains(transfer_digest));

        for (obj_ref, owner) in effects.mutated().iter() {
            assert_ne!(obj_ref.0, child.0 .0);
            if obj_ref.0 == parent.0 .0 {
                // owner of the parent stays the same
                assert_eq!(owner, &parent.1);
                // parent version is also bumped
                assert!(obj_ref.1 > parent.0 .1);
                // child version is the largest in this transaction, and even though it's not received
                // it still contributes to the lamport version of the transaction.
                assert_eq!(obj_ref.1.value(), child.0 .1.value() + 1);
            }
        }
    }
    }
}

#[tokio::test]
async fn test_tto_dependencies_receive_and_abort() {
    transfer_test_runner! {gas_objects: 3, |mut runner: TestRunner| async move {
        let effects = runner
            .run({
                let mut builder = ProgrammableTransactionBuilder::new();
                move_call! {
                    builder,
                    (runner.package.0)::M4::start1()
                };
                builder.finish()
            })
            .await;
        let parent = effects.created()[0].clone();

        let effects = runner
            .run({
                let mut builder = ProgrammableTransactionBuilder::new();
                move_call! {
                    builder,
                    (runner.package.0)::M4::start2()
                };
                builder.finish()
            })
            .await;
        let old_child = effects.created()[0].clone();

        // Use a different gas coin than for all the other transactions. This:
        // 1. Makes sure that we are registering the dependency on the transaction that transferred the
        //    object solely because of the fact that we received it in this transaction.
        // 2. Since the gas coin is fresh it will have a smaller version, so this will test that we
        //    properly compute and update the lamport version that we should use for the transaction.
        let effects = runner
            .run_with_gas_object(
                {
                    let mut builder = ProgrammableTransactionBuilder::new();
                    builder
                        .transfer_object(SuiAddress::from(parent.0 .0), old_child.0)
                        .unwrap();
                    builder.finish()
                },
                1,
            )
            .await;

        let child = effects
            .mutated()
            .iter()
            .find(|(o, _)| o.0 == old_child.0 .0)
            .cloned()
            .unwrap();
        let transfer_digest = effects.transaction_digest();

        assert!(parent.0 .1.value() < child.0 .1.value());

        let effects = runner
            .run_with_gas_object(
                {
                    let mut builder = ProgrammableTransactionBuilder::new();
                    let parent = builder.obj(ObjectArg::ImmOrOwnedObject(parent.0)).unwrap();
                    let child = builder.obj(ObjectArg::Receiving(child.0)).unwrap();
                    move_call! {
                        builder,
                        (runner.package.0)::M4::receive_abort(parent, child)
                    };
                    builder.finish()
                },
                2,
            )
            .await;

        assert!(effects.status().is_err());
        assert!(effects.created().is_empty());
        assert!(effects.unwrapped().is_empty());
        assert!(effects.deleted().is_empty());
        assert!(effects.unwrapped_then_deleted().is_empty());
        assert!(effects.wrapped().is_empty());
        // Received but aborted -- dependency is still added.
        assert!(effects.dependencies().contains(transfer_digest));

        for (obj_ref, owner) in effects.mutated().iter() {
            assert_ne!(obj_ref.0, child.0 .0);
            if obj_ref.0 == parent.0 .0 {
                // owner of the parent stays the same
                assert_eq!(owner, &parent.1);
                // parent version is also bumped
                assert!(obj_ref.1 > parent.0 .1);
                // Child version is the largest in this transaction even though it's not received
                assert_eq!(obj_ref.1.value(), child.0 .1.value() + 1);
            }
        }
    }
    }
}

#[tokio::test]
async fn test_tto_dependencies_receive_and_type_mismatch() {
    transfer_test_runner! {gas_objects: 3, |mut runner: TestRunner| async move {
        let effects = runner
            .run({
                let mut builder = ProgrammableTransactionBuilder::new();
                move_call! {
                    builder,
                    (runner.package.0)::M4::start1()
                };
                builder.finish()
            })
            .await;
        let parent = effects.created()[0].clone();

        let effects = runner
            .run({
                let mut builder = ProgrammableTransactionBuilder::new();
                move_call! {
                    builder,
                    (runner.package.0)::M4::start2()
                };
                builder.finish()
            })
            .await;
        let old_child = effects.created()[0].clone();

        // Use a different gas coin than for all the other transactions. This:
        // 1. Makes sure that we are registering the dependency on the transaction that transferred the
        //    object solely because of the fact that we received it in this transaction.
        // 2. Since the gas coin is fresh it will have a smaller version, so this will test that we
        //    properly compute and update the lamport version that we should use for the transaction.
        let effects = runner
            .run_with_gas_object(
                {
                    let mut builder = ProgrammableTransactionBuilder::new();
                    builder
                        .transfer_object(SuiAddress::from(parent.0 .0), old_child.0)
                        .unwrap();
                    builder.finish()
                },
                1,
            )
            .await;

        let child = effects
            .mutated()
            .iter()
            .find(|(o, _)| o.0 == old_child.0 .0)
            .cloned()
            .unwrap();
        let transfer_digest = effects.transaction_digest();

        assert!(parent.0 .1.value() < child.0 .1.value());

        let effects = runner
            .run_with_gas_object(
                {
                    let mut builder = ProgrammableTransactionBuilder::new();
                    let parent = builder.obj(ObjectArg::ImmOrOwnedObject(parent.0)).unwrap();
                    let child = builder.obj(ObjectArg::Receiving(child.0)).unwrap();
                    move_call! {
                        builder,
                        (runner.package.0)::M4::receive_type_mismatch(parent, child)
                    };
                    builder.finish()
                },
                2,
            )
            .await;

        assert!(effects.status().is_err());

        // Type mismatch is an abort code of 2 from `receive_impl`
        let is_type_mismatch_error = matches!(
            effects.status().clone().unwrap_err().0,
            ExecutionFailureStatus::MoveAbort(x, 2) if x.function_name == Some("receive_impl".to_string())
        );
        assert!(is_type_mismatch_error);
        assert!(effects.created().is_empty());
        assert!(effects.unwrapped().is_empty());
        assert!(effects.deleted().is_empty());
        assert!(effects.unwrapped_then_deleted().is_empty());
        assert!(effects.wrapped().is_empty());
        // Received but there was a type mismatch -- dependency is still added.
        assert!(effects.dependencies().contains(transfer_digest));

        for (obj_ref, owner) in effects.mutated().iter() {
            assert_ne!(obj_ref.0, child.0 .0);
            if obj_ref.0 == parent.0 .0 {
                // owner of the parent stays the same
                assert_eq!(owner, &parent.1);
                // parent version is also bumped
                assert!(obj_ref.1 > parent.0 .1);
                // Child version is the largest in this transaction even though it's not received
                assert_eq!(obj_ref.1.value(), child.0 .1.value() + 1);
            }
        }
    }
    }
}

#[tokio::test]
async fn receive_and_dof_interleave() {
    transfer_test_runner! {gas_objects: 3, |mut runner: TestRunner| async move {
        // step 1 & 2
        let effects = runner
            .run_with_gas_object(
                {
                    let mut builder = ProgrammableTransactionBuilder::new();
                    move_call! {
                        builder,
                        (runner.package.0)::M5::start()
                    };
                    builder.finish()
                },
                0,
            )
            .await;

        let shared = effects
            .created()
            .iter()
            .find(|(_, owner)| matches!(owner, Owner::Shared { .. }))
            .cloned()
            .unwrap();
        let owned = effects
            .created()
            .iter()
            .find(|(_, owner)| matches!(owner, Owner::AddressOwner(_)))
            .cloned()
            .unwrap();
        let Owner::Shared { initial_shared_version } = shared.1 else { unreachable!() };

        let init_digest = effects.transaction_digest();

        let cert = runner
            .lock_and_verify_transaction(
                {
                    let mut builder = ProgrammableTransactionBuilder::new();
                    let parent = builder
                        .obj(ObjectArg::SharedObject {
                            id: shared.0 .0,
                            initial_shared_version,
                            mutable: true,
                        })
                        .unwrap();
                    let child = builder.obj(ObjectArg::Receiving(owned.0)).unwrap();
                    move_call! {
                        builder,
                        (runner.package.0)::M5::deleter(parent, child)
                    };
                    builder.finish()
                },
                1,
            )
            .await;

        let dof_effects = runner
            .run_with_gas_object_shared(
                {
                    let mut builder = ProgrammableTransactionBuilder::new();
                    let parent = builder
                        .obj(ObjectArg::SharedObject {
                            id: shared.0 .0,
                            initial_shared_version,
                            mutable: true,
                        })
                        .unwrap();
                    let child = builder.obj(ObjectArg::ImmOrOwnedObject(owned.0)).unwrap();
                    move_call! {
                        builder,
                        (runner.package.0)::M5::add_dof(parent, child)
                    };
                    builder.finish()
                },
                2,
            )
            .await;

        assert!(dof_effects.status().is_ok());

        let recv_effects = runner.execute_certificate(cert, true).await;
        assert!(recv_effects.status().is_ok());
        // The recv_effects should not contain the dependency on the initial transaction since we
        // didn't actually receive the object -- it was loaded via the dynamic field instead.
        assert!(!recv_effects.dependencies().contains(init_digest));
    }
    }
}

#[tokio::test]
async fn test_have_deleted_owned_object() {
    transfer_test_runner! { |mut runner: TestRunner| async move {
        let effects = runner
            .run({
                let mut builder = ProgrammableTransactionBuilder::new();
                move_call! {
                    builder,
                    (runner.package.0)::M1::start()
                };
                builder.finish()
            })
            .await;

        let (parent, child) = get_parent_and_child(effects.created());

        let effects = runner
            .run({
                let mut builder = ProgrammableTransactionBuilder::new();
                let parent = builder.obj(ObjectArg::ImmOrOwnedObject(parent.0)).unwrap();
                let child = builder.obj(ObjectArg::Receiving(child.0)).unwrap();
                move_call! {
                    builder,
                    (runner.package.0)::M1::send_back(parent, child)
                };
                builder.finish()
            })
            .await;

        let (new_parent, new_child) = get_parent_and_child(effects.mutated());

        let cache = runner.authority_state.get_object_cache_reader().clone();

        assert!(cache.get_object(&new_child.0.0).is_some());
        // Should not show as deleted for either versions
        assert!(!cache.have_deleted_fastpath_object_at_version_or_after(new_child.0.0, new_child.0.1, 0, true));
        assert!(!cache.have_deleted_fastpath_object_at_version_or_after(new_child.0.0, child.0.1, 0, true));

        let effects = runner
            .run({
                let mut builder = ProgrammableTransactionBuilder::new();
                let parent = builder.obj(ObjectArg::ImmOrOwnedObject(new_parent.0)).unwrap();
                let child = builder.obj(ObjectArg::Receiving(new_child.0)).unwrap();
                move_call! {
                    builder,
                    (runner.package.0)::M1::deleter(parent, child)
                };
                builder.finish()
            })
            .await;

        let deleted_child = effects.deleted().into_iter().find(|(id, _, _)| *id == new_child.0 .0).unwrap();
        assert!(cache.get_object(&deleted_child.0).is_none());
        assert!(cache.have_deleted_fastpath_object_at_version_or_after(deleted_child.0, deleted_child.1, 0, true));
        assert!(cache.have_deleted_fastpath_object_at_version_or_after(deleted_child.0, new_child.0.1, 0, true));
        assert!(cache.have_deleted_fastpath_object_at_version_or_after(deleted_child.0, child.0.1, 0, true));
        // Should not show as deleted for versions after this though
        assert!(!cache.have_deleted_fastpath_object_at_version_or_after(deleted_child.0, deleted_child.1.next(), 0, true));
        // Should not show as deleted for other epochs outside of our current epoch too
        assert!(!cache.have_deleted_fastpath_object_at_version_or_after(deleted_child.0, deleted_child.1, 1, true));
    }
    }
}
