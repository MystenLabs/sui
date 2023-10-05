// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

use std::path::PathBuf;
use std::sync::Arc;
use sui_core::authority::authority_per_epoch_store::AuthorityPerEpochStore;
use sui_core::authority::test_authority_builder::TestAuthorityBuilder;
use sui_core::authority::AuthorityState;
use sui_core::authority_server::{ValidatorService, ValidatorServiceMetrics};
use sui_core::consensus_adapter::{
    ConnectionMonitorStatusForTests, ConsensusAdapter, ConsensusAdapterMetrics, SubmitToConsensus,
};
use sui_test_transaction_builder::TestTransactionBuilder;
use sui_types::base_types::{ObjectID, ObjectRef, SuiAddress};
use sui_types::committee::Committee;
use sui_types::crypto::AccountKeyPair;
use sui_types::effects::{TransactionEffects, TransactionEffectsAPI};
use sui_types::error::SuiResult;
use sui_types::executable_transaction::VerifiedExecutableTransaction;
use sui_types::messages_consensus::ConsensusTransaction;
use sui_types::object::Object;
use sui_types::transaction::{
    CertifiedTransaction, Transaction, VerifiedCertificate, VerifiedTransaction,
    DEFAULT_VALIDATOR_GAS_PRICE,
};

#[derive(Clone)]
pub struct SingleValidator {
    validator_state: ValidatorState,
    epoch_store: Arc<AuthorityPerEpochStore>,
}

#[derive(Clone)]
enum ValidatorState {
    EndToEnd(Arc<ValidatorService>),
    Direct(Arc<AuthorityState>),
}

impl ValidatorState {
    fn get_validator(&self) -> &Arc<AuthorityState> {
        match self {
            ValidatorState::EndToEnd(validator) => validator.validator_state(),
            ValidatorState::Direct(validator) => validator,
        }
    }
}

impl SingleValidator {
    pub async fn new(genesis_objects: &[Object], end_to_end: bool) -> Self {
        let validator = TestAuthorityBuilder::new()
            .disable_indexer()
            .with_starting_objects(genesis_objects)
            .build()
            .await;
        let epoch_store = validator.epoch_store_for_testing().clone();
        let validator_state = if end_to_end {
            struct SubmitNoop {}

            #[async_trait::async_trait]
            impl SubmitToConsensus for SubmitNoop {
                async fn submit_to_consensus(
                    &self,
                    _transaction: &ConsensusTransaction,
                    _epoch_store: &Arc<AuthorityPerEpochStore>,
                ) -> SuiResult {
                    Ok(())
                }
            }

            let consensus_adapter = Arc::new(ConsensusAdapter::new(
                Box::new(SubmitNoop {}),
                validator.name,
                Box::new(Arc::new(ConnectionMonitorStatusForTests {})),
                100_000,
                100_000,
                None,
                None,
                ConsensusAdapterMetrics::new_test(),
                epoch_store.protocol_config().clone(),
            ));
            ValidatorState::EndToEnd(Arc::new(ValidatorService::new(
                validator,
                consensus_adapter,
                Arc::new(ValidatorServiceMetrics::new_for_tests()),
            )))
        } else {
            ValidatorState::Direct(validator)
        };
        Self {
            validator_state,
            epoch_store,
        }
    }

    pub async fn get_latest_object_ref(&self, object_id: &ObjectID) -> ObjectRef {
        self.get_validator()
            .get_object(object_id)
            .await
            .unwrap()
            .unwrap()
            .compute_object_reference()
    }

    pub async fn execute_tx_immediately(&self, transaction: Transaction) -> TransactionEffects {
        let executable = VerifiedExecutableTransaction::new_from_quorum_execution(
            VerifiedTransaction::new_unchecked(transaction),
            0,
        );
        let effects = self
            .get_validator()
            .try_execute_immediately(&executable, None, &self.epoch_store)
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
        let effects = self.execute_tx_immediately(transaction).await;
        effects
            .all_changed_objects()
            .into_iter()
            .filter_map(|(oref, owner, _)| owner.is_immutable().then_some(oref))
            .next()
            .unwrap()
    }

    pub async fn execute_transaction(&self, cert: CertifiedTransaction) -> TransactionEffects {
        let effects = match &self.validator_state {
            ValidatorState::EndToEnd(validator) => {
                let response = validator.execute_certificate_for_testing(cert).await;
                response.signed_effects.into_data()
            }
            ValidatorState::Direct(validator) => {
                let cert = VerifiedExecutableTransaction::new_from_certificate(
                    VerifiedCertificate::new_unchecked(cert),
                );
                validator
                    .try_execute_immediately(&cert, None, &self.epoch_store)
                    .await
                    .unwrap()
                    .0
            }
        };
        assert!(effects.status().is_ok());
        effects
    }

    pub fn get_validator(&self) -> &Arc<AuthorityState> {
        self.validator_state.get_validator()
    }

    pub fn get_committee(&self) -> &Committee {
        self.epoch_store.committee()
    }
}
