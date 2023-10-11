// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

use crate::command::Component;
use crate::mock_consensus::{ConsensusMode, MockConsensusClient};
use std::path::PathBuf;
use std::sync::Arc;
use sui_core::authority::authority_per_epoch_store::AuthorityPerEpochStore;
use sui_core::authority::test_authority_builder::TestAuthorityBuilder;
use sui_core::authority::AuthorityState;
use sui_core::authority_server::{ValidatorService, ValidatorServiceMetrics};
use sui_core::consensus_adapter::{
    ConnectionMonitorStatusForTests, ConsensusAdapter, ConsensusAdapterMetrics,
};
use sui_test_transaction_builder::TestTransactionBuilder;
use sui_types::base_types::{ObjectID, ObjectRef, SuiAddress};
use sui_types::committee::Committee;
use sui_types::crypto::AccountKeyPair;
use sui_types::effects::{TransactionEffects, TransactionEffectsAPI};
use sui_types::executable_transaction::VerifiedExecutableTransaction;
use sui_types::object::Object;
use sui_types::transaction::{
    CertifiedTransaction, Transaction, VerifiedCertificate, VerifiedTransaction,
    DEFAULT_VALIDATOR_GAS_PRICE,
};

#[derive(Clone)]
pub struct SingleValidator {
    validator_service: Arc<ValidatorService>,
    epoch_store: Arc<AuthorityPerEpochStore>,
}

impl SingleValidator {
    pub(crate) async fn new(genesis_objects: &[Object], consensus_mode: ConsensusMode) -> Self {
        let validator = TestAuthorityBuilder::new()
            .disable_indexer()
            .with_starting_objects(genesis_objects)
            .build()
            .await;
        let epoch_store = validator.epoch_store_for_testing().clone();
        let consensus_adapter = Arc::new(ConsensusAdapter::new(
            Box::new(MockConsensusClient::new(validator.clone(), consensus_mode)),
            validator.name,
            Box::new(Arc::new(ConnectionMonitorStatusForTests {})),
            100_000,
            100_000,
            None,
            None,
            ConsensusAdapterMetrics::new_test(),
        ));
        let validator_service = Arc::new(ValidatorService::new(
            validator,
            consensus_adapter,
            Arc::new(ValidatorServiceMetrics::new_for_tests()),
        ));
        Self {
            validator_service,
            epoch_store,
        }
    }

    pub fn get_validator(&self) -> &Arc<AuthorityState> {
        self.validator_service.validator_state()
    }

    pub fn get_committee(&self) -> &Committee {
        self.epoch_store.committee()
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

    pub async fn execute_transaction(
        &self,
        cert: CertifiedTransaction,
        component: Component,
    ) -> TransactionEffects {
        let effects = match component {
            Component::Baseline => {
                let cert = VerifiedExecutableTransaction::new_from_certificate(
                    VerifiedCertificate::new_unchecked(cert),
                );
                self.get_validator()
                    .try_execute_immediately(&cert, None, &self.epoch_store)
                    .await
                    .unwrap()
                    .0
            }
            Component::WithTxManager => {
                let cert = VerifiedCertificate::new_unchecked(cert);
                self.get_validator()
                    .execute_certificate(&cert, &self.epoch_store)
                    .await
                    .unwrap()
                    .into_inner()
                    .into_data()
            }
            Component::ValidatorWithoutConsensus | Component::ValidatorWithFakeConsensus => {
                let response = self
                    .validator_service
                    .execute_certificate_for_testing(cert)
                    .await;
                response.signed_effects.into_data()
            }
        };
        assert!(effects.status().is_ok());
        effects
    }
}
