// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

use std::path::PathBuf;
use std::sync::Arc;
use sui_core::authority::authority_per_epoch_store::AuthorityPerEpochStore;
use sui_core::authority::test_authority_builder::TestAuthorityBuilder;
use sui_core::authority::AuthorityState;
use sui_test_transaction_builder::TestTransactionBuilder;
use sui_types::base_types::{ObjectID, ObjectRef, SuiAddress};
use sui_types::crypto::AccountKeyPair;
use sui_types::effects::{TransactionEffects, TransactionEffectsAPI};
use sui_types::executable_transaction::VerifiedExecutableTransaction;
use sui_types::object::Object;
use sui_types::transaction::{VerifiedTransaction, DEFAULT_VALIDATOR_GAS_PRICE};

#[derive(Clone)]
pub struct SingleValidator {
    validator: Arc<AuthorityState>,
    epoch_store: Arc<AuthorityPerEpochStore>,
}

impl SingleValidator {
    pub async fn new(genesis_objects: &[Object]) -> Self {
        let validator = TestAuthorityBuilder::new()
            .disable_indexer()
            .with_starting_objects(genesis_objects)
            .build()
            .await;
        let epoch_store = validator.epoch_store_for_testing().clone();
        Self {
            validator,
            epoch_store,
        }
    }

    pub async fn get_latest_object_ref(&self, object_id: &ObjectID) -> ObjectRef {
        self.validator
            .get_object(object_id)
            .await
            .unwrap()
            .unwrap()
            .compute_object_reference()
    }

    pub async fn execute_tx_immediately(
        &self,
        cert: VerifiedExecutableTransaction,
    ) -> TransactionEffects {
        let effects = self
            .validator
            .try_execute_immediately(&cert, None, &self.epoch_store)
            .await
            .unwrap()
            .0;
        assert!(effects.status().is_ok());
        effects
    }

    pub async fn publish_package(
        &self,
        path: PathBuf,
        sender: SuiAddress,
        keypair: &AccountKeyPair,
        gas: ObjectRef,
    ) -> ObjectRef {
        let transaction = TestTransactionBuilder::new(sender, gas, DEFAULT_VALIDATOR_GAS_PRICE)
            .publish(path)
            .build_and_sign(keypair);
        let effects = self
            .execute_tx_immediately(VerifiedExecutableTransaction::new_from_quorum_execution(
                VerifiedTransaction::new_unchecked(transaction),
                0,
            ))
            .await;
        effects
            .all_changed_objects()
            .into_iter()
            .filter_map(|(oref, owner, _)| owner.is_immutable().then_some(oref))
            .next()
            .unwrap()
    }
}
