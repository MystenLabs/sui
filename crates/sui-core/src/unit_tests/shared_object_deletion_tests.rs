// Copyright (c) 2021, Facebook, Inc. and its affiliates
// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

use std::sync::Arc;

use sui_types::{
    base_types::{ObjectID, ObjectRef, SequenceNumber, SuiAddress},
    crypto::{get_key_pair, AccountKeyPair},
    effects::TransactionEffects,
    object::Object,
    programmable_transaction_builder::ProgrammableTransactionBuilder,
    transaction::{ProgrammableTransaction, Transaction, TEST_ONLY_GAS_UNIT_FOR_PUBLISH},
};

use crate::authority::authority_test_utils::execute_sequenced_certificate_to_effects;
use crate::{
    authority::{
        authority_tests::{
            build_programmable_transaction, certify_shared_obj_transaction_no_execution,
            enqueue_all_and_execute_all, execute_programmable_transaction,
        },
        move_integration_tests::build_and_publish_test_package,
        test_authority_builder::TestAuthorityBuilder,
        AuthorityState,
    },
    move_call,
};
use move_core_types::ident_str;
use sui_protocol_config::{Chain, ProtocolConfig, ProtocolVersion};
use sui_types::base_types::TransactionDigest;
use sui_types::committee::EpochId;
use sui_types::effects::TransactionEffectsAPI;
use sui_types::error::{ExecutionError, SuiError, UserInputError};
use sui_types::execution_status::ExecutionFailureStatus::{
    InputObjectDeleted, MoveAbort, SharedObjectOperationNotAllowed,
};
use sui_types::transaction::{ObjectArg, VerifiedCertificate};

pub struct TestRunner {
    pub sender: SuiAddress,
    pub sender_key: AccountKeyPair,
    pub gas_object_ids: Vec<ObjectID>,
    pub authority_state: Arc<AuthorityState>,
    pub package: ObjectRef,
}

impl TestRunner {
    pub async fn new(base_package_name: &str) -> Self {
        telemetry_subscribers::init_for_testing();
        let (sender, sender_key): (_, AccountKeyPair) = get_key_pair();

        let mut protocol_config =
            ProtocolConfig::get_for_version(ProtocolVersion::max(), Chain::Unknown);
        protocol_config.set_shared_object_deletion(true);
        let authority_state = TestAuthorityBuilder::new()
            .with_protocol_config(protocol_config)
            .build()
            .await;

        let mut gas_object_ids = vec![];
        for _ in 0..5 {
            let gas_object_id = ObjectID::random();
            let gas_object = Object::with_id_owner_for_testing(gas_object_id, sender);
            authority_state.insert_genesis_object(gas_object).await;
            gas_object_ids.push(gas_object_id);
        }

        let package = build_and_publish_test_package(
            &authority_state,
            &sender,
            &sender_key,
            &gas_object_ids[0],
            base_package_name,
            false,
        )
        .await;

        Self {
            sender,
            sender_key,
            gas_object_ids,
            authority_state,
            package,
        }
    }

    pub async fn create_shared_object(&mut self) -> TransactionEffects {
        self.execute_owned_transaction({
            let mut builder = ProgrammableTransactionBuilder::new();
            move_call! {
                builder,
                (self.package.0)::o2::create()
            };
            builder.finish()
        })
        .await
    }

    pub async fn create_owned_object(&mut self) -> TransactionEffects {
        self.execute_owned_transaction({
            let mut builder = ProgrammableTransactionBuilder::new();
            move_call! {
                builder,
                (self.package.0)::o2::create_owned()
            };
            builder.finish()
        })
        .await
    }

    pub fn get_object_latest_version(&mut self, obj_id: ObjectID) -> SequenceNumber {
        self.authority_state
            .database
            .perpetual_tables
            .get_latest_object_ref_or_tombstone(obj_id)
            .unwrap()
            .unwrap()
            .1
    }

    pub async fn execute_owned_transaction(
        &mut self,
        pt: ProgrammableTransaction,
    ) -> TransactionEffects {
        execute_programmable_transaction(
            &self.authority_state,
            &self.gas_object_ids[0],
            &self.sender,
            &self.sender_key,
            pt,
            TEST_ONLY_GAS_UNIT_FOR_PUBLISH,
        )
        .await
        .unwrap()
    }

    pub async fn delete_shared_obj_tx(
        &mut self,
        shared_obj_id: ObjectID,
        initial_shared_version: SequenceNumber,
    ) -> Transaction {
        let mut delete_object_transaction_builder = ProgrammableTransactionBuilder::new();
        let arg = delete_object_transaction_builder
            .obj(ObjectArg::SharedObject {
                id: shared_obj_id,
                initial_shared_version,
                mutable: true,
            })
            .unwrap();
        move_call! {
            delete_object_transaction_builder,
            (self.package.0)::o2::consume_o2(arg)
        };
        let delete_obj_tx = delete_object_transaction_builder.finish();
        let gas_id = self.gas_object_ids.pop().unwrap();
        self.create_signed_transaction_from_pt(delete_obj_tx, gas_id)
            .await
    }

    pub async fn delete_shared_obj_with_owned_tx(
        &mut self,
        owned_obj: ObjectRef,
        shared_obj_id: ObjectID,
        initial_shared_version: SequenceNumber,
    ) -> Transaction {
        let mut object_transaction_builder = ProgrammableTransactionBuilder::new();
        let arg_1 = object_transaction_builder
            .obj(ObjectArg::ImmOrOwnedObject(owned_obj))
            .unwrap();
        let arg_2 = object_transaction_builder
            .obj(ObjectArg::SharedObject {
                id: shared_obj_id,
                initial_shared_version,
                mutable: true,
            })
            .unwrap();

        move_call! {
            object_transaction_builder,
            (self.package.0)::o2::consume_with_owned(arg_1, arg_2)
        };
        let delete_obj_tx = object_transaction_builder.finish();
        let gas_id = self.gas_object_ids.pop().unwrap();
        self.create_signed_transaction_from_pt(delete_obj_tx, gas_id)
            .await
    }

    pub async fn mutate_shared_obj_with_owned_tx(
        &mut self,
        owned_obj: ObjectRef,
        shared_obj_id: ObjectID,
        initial_shared_version: SequenceNumber,
    ) -> Transaction {
        let mut delete_object_transaction_builder = ProgrammableTransactionBuilder::new();
        let arg_1 = delete_object_transaction_builder
            .obj(ObjectArg::ImmOrOwnedObject(owned_obj))
            .unwrap();
        let arg_2 = delete_object_transaction_builder
            .obj(ObjectArg::SharedObject {
                id: shared_obj_id,
                initial_shared_version,
                mutable: true,
            })
            .unwrap();
        move_call! {
            delete_object_transaction_builder,
            (self.package.0)::o2::mutate_with_owned(arg_1, arg_2)
        };
        let delete_obj_tx = delete_object_transaction_builder.finish();
        let gas_id = self.gas_object_ids.pop().unwrap();
        self.create_signed_transaction_from_pt(delete_obj_tx, gas_id)
            .await
    }

    pub async fn vec_delete_obj_tx(
        &mut self,
        shared_obj_id: ObjectID,
        initial_shared_version: SequenceNumber,
    ) -> Transaction {
        let mut delete_object_transaction_builder = ProgrammableTransactionBuilder::new();
        let arg = delete_object_transaction_builder
            .make_obj_vec(vec![ObjectArg::SharedObject {
                id: shared_obj_id,
                initial_shared_version,
                mutable: true,
            }])
            .unwrap();
        move_call! {
            delete_object_transaction_builder,
            (self.package.0)::o2::vec_delete(arg)
        };
        let delete_obj_tx = delete_object_transaction_builder.finish();
        let gas_id = self.gas_object_ids.pop().unwrap();
        self.create_signed_transaction_from_pt(delete_obj_tx, gas_id)
            .await
    }

    pub async fn mutate_shared_obj_tx(
        &mut self,
        shared_obj_id: ObjectID,
        initial_shared_version: SequenceNumber,
    ) -> Transaction {
        let mut delete_object_transaction_builder = ProgrammableTransactionBuilder::new();
        let arg = delete_object_transaction_builder
            .obj(ObjectArg::SharedObject {
                id: shared_obj_id,
                initial_shared_version,
                mutable: true,
            })
            .unwrap();
        move_call! {
            delete_object_transaction_builder,
            (self.package.0)::o2::mutate_o2(arg)
        };
        let delete_obj_tx = delete_object_transaction_builder.finish();
        let gas_id = self.gas_object_ids.pop().unwrap();
        self.create_signed_transaction_from_pt(delete_obj_tx, gas_id)
            .await
    }

    pub async fn wrap_shared_obj_tx(
        &mut self,
        shared_obj_id: ObjectID,
        initial_shared_version: SequenceNumber,
    ) -> Transaction {
        let mut delete_object_transaction_builder = ProgrammableTransactionBuilder::new();
        let arg = delete_object_transaction_builder
            .obj(ObjectArg::SharedObject {
                id: shared_obj_id,
                initial_shared_version,
                mutable: true,
            })
            .unwrap();
        move_call! {
            delete_object_transaction_builder,
            (self.package.0)::o2::wrap_o2(arg)
        };
        let delete_obj_tx = delete_object_transaction_builder.finish();
        let gas_id = self.gas_object_ids.pop().unwrap();
        self.create_signed_transaction_from_pt(delete_obj_tx, gas_id)
            .await
    }

    pub async fn transfer_to_single_owner_tx(
        &mut self,
        shared_obj_id: ObjectID,
        initial_shared_version: SequenceNumber,
    ) -> Transaction {
        let mut delete_object_transaction_builder = ProgrammableTransactionBuilder::new();
        let arg = delete_object_transaction_builder
            .obj(ObjectArg::SharedObject {
                id: shared_obj_id,
                initial_shared_version,
                mutable: true,
            })
            .unwrap();
        move_call! {
            delete_object_transaction_builder,
            (self.package.0)::o2::transfer_to_single_owner(arg)
        };
        let delete_obj_tx = delete_object_transaction_builder.finish();
        let gas_id = self.gas_object_ids.pop().unwrap();
        self.create_signed_transaction_from_pt(delete_obj_tx, gas_id)
            .await
    }

    pub async fn freeze_shared_obj_tx(
        &mut self,
        shared_obj_id: ObjectID,
        initial_shared_version: SequenceNumber,
    ) -> Transaction {
        let mut delete_object_transaction_builder = ProgrammableTransactionBuilder::new();
        let arg = delete_object_transaction_builder
            .obj(ObjectArg::SharedObject {
                id: shared_obj_id,
                initial_shared_version,
                mutable: true,
            })
            .unwrap();
        move_call! {
            delete_object_transaction_builder,
            (self.package.0)::o2::freeze_o2(arg)
        };
        let delete_obj_tx = delete_object_transaction_builder.finish();
        let gas_id = self.gas_object_ids.pop().unwrap();
        self.create_signed_transaction_from_pt(delete_obj_tx, gas_id)
            .await
    }

    pub async fn create_signed_transaction_from_pt(
        &mut self,
        pt: ProgrammableTransaction,
        account_id: ObjectID,
    ) -> Transaction {
        build_programmable_transaction(
            &self.authority_state,
            &account_id,
            &self.sender,
            &self.sender_key,
            pt,
            TEST_ONLY_GAS_UNIT_FOR_PUBLISH,
        )
        .await
        .unwrap()
    }

    pub async fn certify_shared_obj_transaction(
        &mut self,
        tx: Transaction,
    ) -> Result<VerifiedCertificate, SuiError> {
        certify_shared_obj_transaction_no_execution(&self.authority_state, tx).await
    }

    pub async fn enqueue_all_and_execute_all(
        &mut self,
        certificates: Vec<VerifiedCertificate>,
    ) -> Result<Vec<TransactionEffects>, SuiError> {
        enqueue_all_and_execute_all(&self.authority_state, certificates).await
    }

    pub async fn execute_sequenced_certificate_to_effects(
        &mut self,
        certificate: VerifiedCertificate,
    ) -> Result<(TransactionEffects, Option<ExecutionError>), SuiError> {
        execute_sequenced_certificate_to_effects(&self.authority_state, certificate).await
    }

    pub fn object_exists_in_marker_table(
        &mut self,
        object_id: &ObjectID,
        epoch: EpochId,
    ) -> Option<TransactionDigest> {
        self.authority_state
            .database
            .shared_object_deleted(object_id, epoch)
            .unwrap()
    }
}

#[tokio::test]
async fn test_delete_shared_object() {
    let mut user1 = TestRunner::new("shared_object_deletion").await;
    let effects = user1.create_shared_object().await;

    assert_eq!(effects.created().len(), 1);

    let shared_obj = effects.created()[0].0;
    let shared_obj_id = shared_obj.0;
    let initial_shared_version = shared_obj.1;
    let delete_obj_tx = user1
        .delete_shared_obj_tx(shared_obj_id, initial_shared_version)
        .await;

    let cert = user1
        .certify_shared_obj_transaction(delete_obj_tx)
        .await
        .unwrap();

    let (effects, error) = user1
        .execute_sequenced_certificate_to_effects(cert)
        .await
        .unwrap();

    assert!(error.is_none());

    assert_eq!(effects.deleted().len(), 1);

    // assert the shared object was deleted
    let deleted_obj_id = effects.deleted()[0].0;
    let shared_obj_id = effects.input_shared_objects()[0].0 .0;
    assert_eq!(deleted_obj_id, shared_obj_id);

    // assert the version of the deleted shared object was incremented
    let deleted_obj_ver = effects.deleted()[0].1;
    assert_eq!(deleted_obj_ver, 4.into());

    // assert the rest of the effects are as expected
    assert!(effects.status().is_ok());
    assert!(effects.created().is_empty());
    assert!(effects.unwrapped_then_deleted().is_empty());
    assert!(effects.wrapped().is_empty());

    assert_eq!(
        user1
            .object_exists_in_marker_table(&deleted_obj_id, 0)
            .unwrap(),
        *effects.transaction_digest(),
    );

    assert_eq!(
        user1
            .object_exists_in_marker_table(&deleted_obj_id, 0)
            .unwrap(),
        *effects.transaction_digest(),
    );
}

#[tokio::test]
async fn test_mutate_after_delete() {
    let mut user_1 = TestRunner::new("shared_object_deletion").await;
    let effects = user_1.create_shared_object().await;

    assert_eq!(effects.created().len(), 1);

    let shared_obj = effects.created()[0].0;
    let shared_obj_id = shared_obj.0;
    let initial_shared_version = shared_obj.1;

    let mutate_obj_tx = user_1
        .mutate_shared_obj_tx(shared_obj_id, initial_shared_version)
        .await;

    let delete_obj_tx = user_1
        .delete_shared_obj_tx(shared_obj_id, initial_shared_version)
        .await;

    let delete_cert = user_1
        .certify_shared_obj_transaction(delete_obj_tx)
        .await
        .unwrap();

    let mutate_cert = user_1
        .certify_shared_obj_transaction(mutate_obj_tx)
        .await
        .unwrap();

    let (orig_effects, _error) = user_1
        .execute_sequenced_certificate_to_effects(delete_cert)
        .await
        .unwrap();

    let digest = orig_effects.transaction_digest();

    let (effects, error) = user_1
        .execute_sequenced_certificate_to_effects(mutate_cert)
        .await
        .unwrap();

    assert!(matches!(error.unwrap().kind(), InputObjectDeleted));
    assert!(effects.status().is_err());
    assert_eq!(effects.deleted().len(), 0);

    assert!(effects.created().is_empty());
    assert!(effects.unwrapped_then_deleted().is_empty());
    assert!(effects.wrapped().is_empty());

    // The gas coin gets mutated
    assert_eq!(effects.mutated().len(), 1);

    let mut found_digest_in_dependencies = false;
    for dependency in effects.dependencies() {
        if dependency == digest {
            found_digest_in_dependencies = true;
        }
    }

    assert!(found_digest_in_dependencies);
}

#[tokio::test]
async fn test_mutate_after_delete_enqueued() {
    let mut user_1 = TestRunner::new("shared_object_deletion").await;
    let effects = user_1.create_shared_object().await;

    assert_eq!(effects.created().len(), 1);

    let shared_obj = effects.created()[0].0;
    let shared_obj_id = shared_obj.0;
    let initial_shared_version = shared_obj.1;

    let mutate_obj_tx = user_1
        .mutate_shared_obj_tx(shared_obj_id, initial_shared_version)
        .await;

    let mutate_obj_tx_2 = user_1
        .mutate_shared_obj_tx(shared_obj_id, initial_shared_version)
        .await;

    let delete_obj_tx = user_1
        .delete_shared_obj_tx(shared_obj_id, initial_shared_version)
        .await;

    let delete_cert = user_1
        .certify_shared_obj_transaction(delete_obj_tx)
        .await
        .unwrap();

    let mutate_cert = user_1
        .certify_shared_obj_transaction(mutate_obj_tx)
        .await
        .unwrap();

    let mutate_cert_2 = user_1
        .certify_shared_obj_transaction(mutate_obj_tx_2)
        .await
        .unwrap();

    let res = user_1
        .enqueue_all_and_execute_all(vec![delete_cert, mutate_cert, mutate_cert_2])
        .await
        .unwrap();

    let effects = res.get(1).unwrap();

    assert!(effects.status().is_err());
    assert_eq!(effects.deleted().len(), 0);

    assert!(effects.created().is_empty());
    assert!(effects.unwrapped_then_deleted().is_empty());
    assert!(effects.wrapped().is_empty());

    // The gas coin gets mutated
    assert_eq!(effects.mutated().len(), 1);

    let effects = res.get(2).unwrap();
    assert!(effects.status().is_err());
    assert_eq!(effects.deleted().len(), 0);

    assert!(effects.created().is_empty());
    assert!(effects.unwrapped_then_deleted().is_empty());
    assert!(effects.wrapped().is_empty());

    // The gas coin gets mutated
    assert_eq!(effects.mutated().len(), 1);
}

#[tokio::test]
async fn test_wrap_not_allowed() {
    let mut user_1 = TestRunner::new("shared_object_deletion").await;
    let effects = user_1.create_shared_object().await;

    assert_eq!(effects.created().len(), 1);

    let shared_obj = effects.created()[0].0;
    let shared_obj_id = shared_obj.0;
    let initial_shared_version = shared_obj.1;

    let wrap_shared_obj_tx = user_1
        .wrap_shared_obj_tx(shared_obj_id, initial_shared_version)
        .await;

    let wrap_cert = user_1
        .certify_shared_obj_transaction(wrap_shared_obj_tx)
        .await
        .unwrap();

    let (effects, error) = user_1
        .execute_sequenced_certificate_to_effects(wrap_cert)
        .await
        .unwrap();

    assert!(matches!(
        error.unwrap().kind(),
        SharedObjectOperationNotAllowed
    ));

    let new_version = user_1.get_object_latest_version(shared_obj_id);
    assert_eq!(new_version, 4.into());
    assert_eq!(effects.mutated().len(), 2);
}

#[tokio::test]
async fn test_vec_delete() {
    let mut user_1 = TestRunner::new("shared_object_deletion").await;
    let effects = user_1.create_shared_object().await;

    assert_eq!(effects.created().len(), 1);

    let shared_obj = effects.created()[0].0;
    let shared_obj_id = shared_obj.0;
    let initial_shared_version = shared_obj.1;

    let shared_obj_tx = user_1
        .vec_delete_obj_tx(shared_obj_id, initial_shared_version)
        .await;

    let cert = user_1
        .certify_shared_obj_transaction(shared_obj_tx)
        .await
        .unwrap();

    let (_effects, error) = user_1
        .execute_sequenced_certificate_to_effects(cert)
        .await
        .unwrap();

    assert!(error.is_none());
}

#[tokio::test]
async fn test_convert_to_owned_not_allowed() {
    let mut user_1 = TestRunner::new("shared_object_deletion").await;
    let effects = user_1.create_shared_object().await;

    assert_eq!(effects.created().len(), 1);

    let shared_obj = effects.created()[0].0;
    let shared_obj_id = shared_obj.0;
    let initial_shared_version = shared_obj.1;

    let tx = user_1
        .transfer_to_single_owner_tx(shared_obj_id, initial_shared_version)
        .await;

    let cert = user_1.certify_shared_obj_transaction(tx).await.unwrap();

    let (effects, error) = user_1
        .execute_sequenced_certificate_to_effects(cert)
        .await
        .unwrap();

    assert!(matches!(error.unwrap().kind(), MoveAbort(..)));

    let new_version = user_1.get_object_latest_version(shared_obj_id);
    assert_eq!(new_version, 4.into());
    assert_eq!(effects.mutated().len(), 2);
}

#[tokio::test]
async fn test_freeze_not_allowed() {
    let mut user_1 = TestRunner::new("shared_object_deletion").await;
    let effects = user_1.create_shared_object().await;

    assert_eq!(effects.created().len(), 1);

    let shared_obj = effects.created()[0].0;
    let shared_obj_id = shared_obj.0;
    let initial_shared_version = shared_obj.1;

    let tx = user_1
        .freeze_shared_obj_tx(shared_obj_id, initial_shared_version)
        .await;

    let cert = user_1.certify_shared_obj_transaction(tx).await.unwrap();

    let (effects, error) = user_1
        .execute_sequenced_certificate_to_effects(cert)
        .await
        .unwrap();

    assert!(matches!(error.unwrap().kind(), MoveAbort(..)));

    let new_version = user_1.get_object_latest_version(shared_obj_id);
    assert_eq!(new_version, 4.into());
    assert_eq!(effects.mutated().len(), 2);
}

#[tokio::test]
async fn test_deletion_twice() {
    let mut user_1 = TestRunner::new("shared_object_deletion").await;
    let effects = user_1.create_shared_object().await;

    assert_eq!(effects.created().len(), 1);

    let shared_obj = effects.created()[0].0;
    let shared_obj_id = shared_obj.0;
    let initial_shared_version = shared_obj.1;

    let delete_obj_tx = user_1
        .delete_shared_obj_tx(shared_obj_id, initial_shared_version)
        .await;

    let delete_obj_tx_2 = user_1
        .delete_shared_obj_tx(shared_obj_id, initial_shared_version)
        .await;

    let delete_cert = user_1
        .certify_shared_obj_transaction(delete_obj_tx)
        .await
        .unwrap();

    let delete_cert_2 = user_1
        .certify_shared_obj_transaction(delete_obj_tx_2)
        .await
        .unwrap();

    let (_effects, _error) = user_1
        .execute_sequenced_certificate_to_effects(delete_cert)
        .await
        .unwrap();

    let (effects, error) = user_1
        .execute_sequenced_certificate_to_effects(delete_cert_2)
        .await
        .unwrap();

    assert!(matches!(error.unwrap().kind(), InputObjectDeleted));

    let new_version = user_1.get_object_latest_version(shared_obj_id);
    assert_eq!(new_version, 4.into());
    assert_eq!(effects.mutated().len(), 1);
}

#[tokio::test]
async fn test_certs_fail_after_delete() {
    let mut user_1 = TestRunner::new("shared_object_deletion").await;
    let effects = user_1.create_shared_object().await;

    assert_eq!(effects.created().len(), 1);

    let shared_obj = effects.created()[0].0;
    let shared_obj_id = shared_obj.0;
    let initial_shared_version = shared_obj.1;

    let delete_obj_tx = user_1
        .delete_shared_obj_tx(shared_obj_id, initial_shared_version)
        .await;

    let delete_cert = user_1
        .certify_shared_obj_transaction(delete_obj_tx)
        .await
        .unwrap();

    let (_effects, _error) = user_1
        .execute_sequenced_certificate_to_effects(delete_cert)
        .await
        .unwrap();

    let mutate_obj_tx = user_1
        .mutate_shared_obj_tx(shared_obj_id, initial_shared_version)
        .await;

    let mutate_cert_result = user_1.certify_shared_obj_transaction(mutate_obj_tx).await;

    assert!(mutate_cert_result.is_err());

    match mutate_cert_result.err().unwrap() {
        SuiError::UserInputError {
            error: UserInputError::ObjectNotFound { object_id, .. },
        } => {
            assert_eq!(object_id, shared_obj_id);
        }
        _ => {
            panic!("Expected UserInputError ObjectNotFound");
        }
    }
}

#[tokio::test]
async fn test_delete_before_two_mutations() {
    let mut user_1 = TestRunner::new("shared_object_deletion").await;
    let effects = user_1.create_shared_object().await;

    assert_eq!(effects.created().len(), 1);

    let shared_obj = effects.created()[0].0;
    let shared_obj_id = shared_obj.0;
    let initial_shared_version = shared_obj.1;

    // t1 mutates object
    // t2 mutates object
    // t3 deletes object

    // execution order t3 -> t1 -> t2
    let tx_mutate_1 = user_1
        .mutate_shared_obj_tx(shared_obj_id, initial_shared_version)
        .await;

    let tx_mutate_2 = user_1
        .mutate_shared_obj_tx(shared_obj_id, initial_shared_version)
        .await;

    let delete_obj_tx = user_1
        .delete_shared_obj_tx(shared_obj_id, initial_shared_version)
        .await;

    let delete_cert = user_1
        .certify_shared_obj_transaction(delete_obj_tx)
        .await
        .unwrap();

    let mutate_cert_1 = user_1
        .certify_shared_obj_transaction(tx_mutate_1)
        .await
        .unwrap();

    let mutate_cert_2 = user_1
        .certify_shared_obj_transaction(tx_mutate_2)
        .await
        .unwrap();

    let (_effects, _error) = user_1
        .execute_sequenced_certificate_to_effects(delete_cert)
        .await
        .unwrap();

    let (effects, error) = user_1
        .execute_sequenced_certificate_to_effects(mutate_cert_1)
        .await
        .unwrap();

    assert!(matches!(error.unwrap().kind(), InputObjectDeleted));
    assert!(effects.status().is_err());
    assert_eq!(effects.deleted().len(), 0);

    assert!(effects.created().is_empty());
    assert!(effects.unwrapped_then_deleted().is_empty());
    assert!(effects.wrapped().is_empty());

    // The gas coin gets mutated
    assert_eq!(effects.mutated().len(), 1);

    let (effects, error) = user_1
        .execute_sequenced_certificate_to_effects(mutate_cert_2)
        .await
        .unwrap();

    assert!(matches!(error.unwrap().kind(), InputObjectDeleted));
    assert!(effects.status().is_err());
    assert_eq!(effects.deleted().len(), 0);

    assert!(effects.created().is_empty());
    assert!(effects.unwrapped_then_deleted().is_empty());
    assert!(effects.wrapped().is_empty());

    // The gas coin gets mutated
    assert_eq!(effects.mutated().len(), 1);
}

#[tokio::test]
async fn test_object_lock_conflict() {
    let mut user_1 = TestRunner::new("shared_object_deletion").await;
    let effects = user_1.create_shared_object().await;

    assert_eq!(effects.created().len(), 1);
    let shared_obj = effects.created()[0].0;
    let shared_obj_id = shared_obj.0;
    let initial_shared_version = shared_obj.1;

    let owned_effects = user_1.create_owned_object().await;

    assert_eq!(owned_effects.created().len(), 1);
    let owned_obj = owned_effects.created()[0].0;

    let delete_obj_tx = user_1
        .delete_shared_obj_with_owned_tx(owned_obj, shared_obj_id, initial_shared_version)
        .await;

    let _delete_cert = user_1
        .certify_shared_obj_transaction(delete_obj_tx)
        .await
        .unwrap();

    let mutate_obj_tx = user_1
        .mutate_shared_obj_with_owned_tx(owned_obj, shared_obj_id, initial_shared_version)
        .await;

    let mutate_cert_res = user_1.certify_shared_obj_transaction(mutate_obj_tx).await;

    assert!(matches!(
        mutate_cert_res.err(),
        Some(SuiError::ObjectLockConflict { .. })
    ));
}

#[tokio::test]
async fn test_owned_object_version_increments_on_cert_denied() {
    let mut user_1 = TestRunner::new("shared_object_deletion").await;
    let effects = user_1.create_shared_object().await;

    assert_eq!(effects.created().len(), 1);
    let shared_obj = effects.created()[0].0;
    let shared_obj_id = shared_obj.0;
    let initial_shared_version = shared_obj.1;

    let owned_effects = user_1.create_owned_object().await;

    assert_eq!(owned_effects.created().len(), 1);
    let owned_obj = owned_effects.created()[0].0;
    let owned_obj_id = owned_obj.0;

    let delete_obj_tx = user_1
        .delete_shared_obj_tx(shared_obj_id, initial_shared_version)
        .await;

    let delete_cert = user_1
        .certify_shared_obj_transaction(delete_obj_tx)
        .await
        .unwrap();

    let mutate_obj_tx = user_1
        .mutate_shared_obj_with_owned_tx(owned_obj, shared_obj_id, initial_shared_version)
        .await;

    let mutate_cert = user_1
        .certify_shared_obj_transaction(mutate_obj_tx)
        .await
        .unwrap();

    let (TransactionEffects::V1(_delete_effects), _error) = user_1
        .execute_sequenced_certificate_to_effects(delete_cert)
        .await
        .unwrap();

    let version = user_1.get_object_latest_version(owned_obj_id);
    assert_eq!(version, 4.into());

    let (TransactionEffects::V1(_mutate_effects), _error) = user_1
        .execute_sequenced_certificate_to_effects(mutate_cert)
        .await
        .unwrap();

    let next_version = user_1.get_object_latest_version(owned_obj_id);
    assert_eq!(next_version, 5.into());
}

#[tokio::test]
async fn test_interspersed_mutations_with_delete() {
    let mut user_1 = TestRunner::new("shared_object_deletion").await;
    let effects = user_1.create_shared_object().await;

    assert_eq!(effects.created().len(), 1);

    let shared_obj = effects.created()[0].0;
    let shared_obj_id = shared_obj.0;
    let initial_shared_version = shared_obj.1;

    // t1 mutates object
    // t2 mutates object
    // t3 deletes object

    // execution order t1 -> t3 -> t2
    let tx_mutate_1 = user_1
        .mutate_shared_obj_tx(shared_obj_id, initial_shared_version)
        .await;

    let tx_mutate_2 = user_1
        .mutate_shared_obj_tx(shared_obj_id, initial_shared_version)
        .await;

    let delete_obj_tx = user_1
        .delete_shared_obj_tx(shared_obj_id, initial_shared_version)
        .await;

    let mutate_cert_1 = user_1
        .certify_shared_obj_transaction(tx_mutate_1)
        .await
        .unwrap();

    let delete_cert = user_1
        .certify_shared_obj_transaction(delete_obj_tx)
        .await
        .unwrap();

    let mutate_cert_2 = user_1
        .certify_shared_obj_transaction(tx_mutate_2)
        .await
        .unwrap();

    let (_effects, _error) = user_1
        .execute_sequenced_certificate_to_effects(mutate_cert_1)
        .await
        .unwrap();

    let (_effects, _error) = user_1
        .execute_sequenced_certificate_to_effects(delete_cert)
        .await
        .unwrap();

    let (effects, error) = user_1
        .execute_sequenced_certificate_to_effects(mutate_cert_2)
        .await
        .unwrap();

    assert!(matches!(error.unwrap().kind(), InputObjectDeleted));
    assert!(effects.status().is_err());
    assert_eq!(effects.deleted().len(), 0);

    assert!(effects.created().is_empty());
    assert!(effects.unwrapped_then_deleted().is_empty());
    assert!(effects.wrapped().is_empty());

    // The gas coin gets mutated
    assert_eq!(effects.mutated().len(), 1);
}
