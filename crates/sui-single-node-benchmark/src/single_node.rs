// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

use crate::command::Component;
use crate::mock_consensus::{ConsensusMode, MockConsensusClient};
use crate::mock_storage::InMemoryObjectStore;
use std::collections::{BTreeMap, HashMap, HashSet};
use std::path::PathBuf;
use std::sync::Arc;
use sui_core::authority::authority_per_epoch_store::AuthorityPerEpochStore;
use sui_core::authority::authority_store_tables::LiveObject;
use sui_core::authority::test_authority_builder::TestAuthorityBuilder;
use sui_core::authority::AuthorityState;
use sui_core::authority_server::{ValidatorService, ValidatorServiceMetrics};
use sui_core::checkpoints::checkpoint_executor::CheckpointExecutor;
use sui_core::consensus_adapter::{
    ConnectionMonitorStatusForTests, ConsensusAdapter, ConsensusAdapterMetrics,
};
use sui_core::state_accumulator::StateAccumulator;
use sui_test_transaction_builder::TestTransactionBuilder;
use sui_types::base_types::{AuthorityName, ObjectRef, SuiAddress, TransactionDigest};
use sui_types::committee::Committee;
use sui_types::crypto::{AccountKeyPair, AuthoritySignature, Signer};
use sui_types::effects::{TransactionEffects, TransactionEffectsAPI};
use sui_types::executable_transaction::VerifiedExecutableTransaction;
use sui_types::messages_checkpoint::{
    EndOfEpochData, VerifiedCheckpoint, VerifiedCheckpointContents,
};
use sui_types::messages_grpc::HandleTransactionResponse;
use sui_types::mock_checkpoint_builder::{MockCheckpointBuilder, ValidatorKeypairProvider};
use sui_types::object::Object;
use sui_types::transaction::{
    CertifiedTransaction, Transaction, TransactionDataAPI, VerifiedCertificate,
    VerifiedTransaction, DEFAULT_VALIDATOR_GAS_PRICE,
};
use tokio::sync::broadcast;

#[derive(Clone)]
pub struct SingleValidator {
    validator_service: Arc<ValidatorService>,
    epoch_store: Arc<AuthorityPerEpochStore>,
}

impl SingleValidator {
    pub(crate) async fn new(
        genesis_objects: &[Object],
        component: Component,
        checkpoint_size: usize,
    ) -> Self {
        let validator = TestAuthorityBuilder::new()
            .disable_indexer()
            .with_starting_objects(genesis_objects)
            // This is needed to properly run checkpoint executor.
            .insert_genesis_checkpoint()
            .build()
            .await;
        let epoch_store = validator.epoch_store_for_testing().clone();
        let consensus_mode = match component {
            Component::ValidatorWithFakeConsensus => {
                ConsensusMode::DirectSequencing(checkpoint_size)
            }
            _ => ConsensusMode::Noop,
        };
        let consensus_adapter = Arc::new(ConsensusAdapter::new(
            Arc::new(MockConsensusClient::new(validator.clone(), consensus_mode)),
            validator.name,
            Arc::new(ConnectionMonitorStatusForTests {}),
            100_000,
            100_000,
            None,
            None,
            ConsensusAdapterMetrics::new_test(),
            epoch_store.protocol_config().clone(),
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

    pub fn get_epoch_store(&self) -> &Arc<AuthorityPerEpochStore> {
        &self.epoch_store
    }

    /// Publish a package, returns the package object and the updated gas object.
    pub async fn publish_package(
        &self,
        path: PathBuf,
        sender: SuiAddress,
        keypair: &AccountKeyPair,
        gas: ObjectRef,
    ) -> (ObjectRef, ObjectRef) {
        let transaction = TestTransactionBuilder::new(sender, gas, DEFAULT_VALIDATOR_GAS_PRICE)
            .publish(path)
            .build_and_sign(keypair);
        let effects = self.execute_raw_transaction(transaction).await;
        let package = effects
            .all_changed_objects()
            .into_iter()
            .filter_map(|(oref, owner, _)| owner.is_immutable().then_some(oref))
            .next()
            .unwrap();
        let updated_gas = effects.gas_object().0;
        (package, updated_gas)
    }

    pub async fn execute_raw_transaction(&self, transaction: Transaction) -> TransactionEffects {
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

    pub async fn execute_certificate(
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
            Component::TxnSigning | Component::CheckpointExecutor | Component::ExecutionOnly => {
                unreachable!()
            }
        };
        assert!(effects.status().is_ok());
        effects
    }

    pub(crate) async fn execute_transaction_in_memory(
        &self,
        store: InMemoryObjectStore,
        transaction: Transaction,
    ) -> TransactionEffects {
        let tx_digest = transaction.digest();
        let input_objects = transaction.transaction_data().input_objects().unwrap();
        let objects = if transaction
            .data()
            .intent_message()
            .value
            .is_end_of_epoch_tx()
        {
            store
                .read_objects_for_synchronous_execution(&input_objects)
                .unwrap()
        } else {
            store
                .read_objects_for_execution(&*self.epoch_store, tx_digest, &input_objects)
                .unwrap()
        };

        let executable = VerifiedExecutableTransaction::new_from_quorum_execution(
            VerifiedTransaction::new_unchecked(transaction),
            0,
        );
        let (gas_status, input_objects) = sui_transaction_checks::check_certificate_input(
            &executable,
            objects,
            self.epoch_store.protocol_config(),
            self.epoch_store.reference_gas_price(),
        )
        .unwrap();
        let (kind, signer, gas) = executable.transaction_data().execution_parts();
        let (_, effects, _) = self.epoch_store.executor().execute_transaction_to_effects(
            &store,
            self.epoch_store.protocol_config(),
            self.get_validator().metrics.limits_metrics.clone(),
            false,
            &HashSet::new(),
            &self.epoch_store.epoch(),
            0,
            input_objects,
            gas,
            gas_status,
            kind,
            signer,
            *executable.digest(),
        );
        assert!(effects.status().is_ok());
        effects
    }

    pub async fn sign_transaction(&self, transaction: Transaction) -> HandleTransactionResponse {
        self.validator_service
            .handle_transaction_for_testing(transaction)
            .await
    }

    pub(crate) async fn build_checkpoints(
        &self,
        in_memory_store: InMemoryObjectStore,
        transactions: Vec<Transaction>,
        mut all_effects: BTreeMap<TransactionDigest, TransactionEffects>,
        checkpoint_size: usize,
    ) -> Vec<(VerifiedCheckpoint, VerifiedCheckpointContents)> {
        let mut builder = MockCheckpointBuilder::new(
            self.get_validator()
                .get_checkpoint_store()
                .get_latest_certified_checkpoint()
                .unwrap(),
        );
        let mut checkpoints = vec![];
        for transaction in transactions {
            let effects = all_effects.remove(transaction.digest()).unwrap();
            builder.push_transaction(VerifiedTransaction::new_unchecked(transaction), effects);
            if builder.size() == checkpoint_size {
                let (checkpoint, _, full_contents) = builder.build(self, 0);
                checkpoints.push((checkpoint, full_contents));
            }
        }
        let gas_cost_summary = builder.epoch_rolling_gas_cost_summary();
        let epoch_tx = VerifiedTransaction::new_change_epoch(
            1,
            self.epoch_store.protocol_version(),
            gas_cost_summary.storage_cost,
            gas_cost_summary.computation_cost,
            gas_cost_summary.storage_rebate,
            gas_cost_summary.non_refundable_storage_fee,
            0,
            vec![],
        );
        let epoch_effects = self
            .execute_transaction_in_memory(in_memory_store, epoch_tx.clone().into_inner())
            .await;
        builder.push_transaction(epoch_tx, epoch_effects);
        let (checkpoint, _, full_contents) = builder.build_end_of_epoch(
            self,
            0,
            1,
            EndOfEpochData {
                next_epoch_committee: self.get_committee().voting_rights.clone(),
                next_epoch_protocol_version: self.get_epoch_store().protocol_version(),
                epoch_commitments: vec![],
            },
        );
        checkpoints.push((checkpoint, full_contents));
        checkpoints
    }

    pub fn create_checkpoint_executor(
        &self,
    ) -> (CheckpointExecutor, broadcast::Sender<VerifiedCheckpoint>) {
        let validator = self.get_validator();
        let (ckpt_sender, ckpt_receiver) = broadcast::channel(1000000);
        let checkpoint_executor = CheckpointExecutor::new_for_tests(
            ckpt_receiver,
            validator.get_checkpoint_store().clone(),
            validator.clone(),
            Arc::new(StateAccumulator::new(validator.db())),
        );
        (checkpoint_executor, ckpt_sender)
    }

    pub(crate) fn create_in_memory_store(&self) -> InMemoryObjectStore {
        let objects: HashMap<_, _> = self
            .get_validator()
            .database
            .iter_live_object_set(false)
            .map(|o| match o {
                LiveObject::Normal(object) => (object.id(), object),
                LiveObject::Wrapped(_) => unreachable!(),
            })
            .collect();
        InMemoryObjectStore::new(objects)
    }
}

impl ValidatorKeypairProvider for SingleValidator {
    fn get_validator_key(&self, name: &AuthorityName) -> &dyn Signer<AuthoritySignature> {
        assert_eq!(name, &self.get_validator().name);
        &*self.get_validator().secret
    }

    fn get_committee(&self) -> &Committee {
        self.epoch_store.committee().as_ref()
    }
}
