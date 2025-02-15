// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

use crate::command::Component;
use crate::mock_storage::InMemoryObjectStore;
use std::collections::{BTreeMap, HashMap, HashSet};
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
use sui_core::mock_consensus::{ConsensusMode, MockConsensusClient};
use sui_core::state_accumulator::StateAccumulator;
use sui_test_transaction_builder::{PublishData, TestTransactionBuilder};
use sui_types::base_types::{AuthorityName, ObjectRef, SuiAddress, TransactionDigest};
use sui_types::committee::Committee;
use sui_types::crypto::{AccountKeyPair, AuthoritySignature, Signer};
use sui_types::effects::{TransactionEffects, TransactionEffectsAPI};
use sui_types::executable_transaction::VerifiedExecutableTransaction;
use sui_types::messages_checkpoint::{VerifiedCheckpoint, VerifiedCheckpointContents};
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
    pub(crate) async fn new(genesis_objects: &[Object], component: Component) -> Self {
        let validator = TestAuthorityBuilder::new()
            .disable_indexer()
            .with_starting_objects(genesis_objects)
            // This is needed to properly run checkpoint executor.
            .insert_genesis_checkpoint()
            .build()
            .await;
        let epoch_store = validator.epoch_store_for_testing().clone();
        let consensus_mode = match component {
            Component::ValidatorWithFakeConsensus => ConsensusMode::DirectSequencing,
            _ => ConsensusMode::Noop,
        };
        let consensus_adapter = Arc::new(ConsensusAdapter::new(
            Arc::new(MockConsensusClient::new(
                Arc::downgrade(&validator),
                consensus_mode,
            )),
            validator.name,
            Arc::new(ConnectionMonitorStatusForTests {}),
            100_000,
            100_000,
            None,
            None,
            ConsensusAdapterMetrics::new_test(),
            epoch_store.protocol_config().clone(),
        ));
        // TODO: for validator benchmarking purposes, we should allow for traffic control
        // to be configurable and introduce traffic control benchmarks to test
        // against different policies
        let validator_service = Arc::new(ValidatorService::new_for_tests(
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
        publish_data: PublishData,
        sender: SuiAddress,
        keypair: &AccountKeyPair,
        gas: ObjectRef,
    ) -> (ObjectRef, ObjectRef) {
        let tx_builder = TestTransactionBuilder::new(sender, gas, DEFAULT_VALIDATOR_GAS_PRICE)
            .publish_with_data(publish_data);
        let transaction = tx_builder.build_and_sign(keypair);
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

    pub async fn execute_dry_run(&self, transaction: Transaction) -> TransactionEffects {
        let effects = self
            .get_validator()
            .dry_exec_transaction_for_benchmark(
                transaction.data().intent_message().value.clone(),
                *transaction.digest(),
            )
            .unwrap()
            .2;
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
                if cert.contains_shared_object() {
                    // For shared objects transactions, `execute_certificate` won't enqueue it because
                    // it expects consensus to do so. However we don't have consensus, hence the manual enqueue.
                    self.get_validator()
                        .enqueue_certificates_for_execution(vec![cert.clone()], &self.epoch_store);
                }
                self.get_validator()
                    .execute_certificate(&cert, &self.epoch_store)
                    .await
                    .unwrap()
            }
            Component::ValidatorWithoutConsensus | Component::ValidatorWithFakeConsensus => {
                let response = self
                    .validator_service
                    .execute_certificate_for_testing(cert)
                    .await
                    .unwrap()
                    .into_inner();
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
        transaction: CertifiedTransaction,
    ) -> TransactionEffects {
        let input_objects = transaction.transaction_data().input_objects().unwrap();
        let objects = store
            .read_objects_for_execution(&self.epoch_store, &transaction.key(), &input_objects)
            .unwrap();

        let executable = VerifiedExecutableTransaction::new_from_certificate(
            VerifiedCertificate::new_unchecked(transaction),
        );
        let (gas_status, input_objects) = sui_transaction_checks::check_certificate_input(
            &executable,
            objects,
            self.epoch_store.protocol_config(),
            self.epoch_store.reference_gas_price(),
        )
        .unwrap();
        let (kind, signer, gas) = executable.transaction_data().execution_parts();
        let (inner_temp_store, _, effects, _timings, _) =
            self.epoch_store.executor().execute_transaction_to_effects(
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
                &mut None,
            );
        assert!(effects.status().is_ok());
        store.commit_objects(inner_temp_store);
        effects
    }

    pub async fn sign_transaction(&self, transaction: Transaction) -> HandleTransactionResponse {
        self.validator_service
            .handle_transaction_for_benchmarking(transaction)
            .await
            .unwrap()
            .into_inner()
    }

    pub(crate) async fn build_checkpoints(
        &self,
        transactions: Vec<CertifiedTransaction>,
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
            builder.push_transaction(
                VerifiedTransaction::new_unchecked(transaction.into_unsigned()),
                effects,
            );
            if builder.size() == checkpoint_size {
                let (checkpoint, _, full_contents) = builder.build(self, 0);
                checkpoints.push((checkpoint, full_contents));
            }
        }
        if builder.size() > 0 {
            let (checkpoint, _, full_contents) = builder.build(self, 0);
            checkpoints.push((checkpoint, full_contents));
        }
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
            Arc::new(StateAccumulator::new_for_tests(
                validator.get_accumulator_store().clone(),
            )),
        );
        (checkpoint_executor, ckpt_sender)
    }

    pub(crate) fn create_in_memory_store(&self) -> InMemoryObjectStore {
        let objects: HashMap<_, _> = self
            .get_validator()
            .get_accumulator_store()
            .iter_cached_live_object_set_for_testing(false)
            .map(|o| match o {
                LiveObject::Normal(object) => (object.id(), object),
                LiveObject::Wrapped(_) => unreachable!(),
            })
            .collect();
        InMemoryObjectStore::new(objects)
    }

    pub(crate) async fn assigned_shared_object_versions(
        &self,
        transactions: &[CertifiedTransaction],
    ) {
        let transactions: Vec<_> = transactions
            .iter()
            .map(|tx| {
                VerifiedExecutableTransaction::new_from_certificate(
                    VerifiedCertificate::new_unchecked(tx.clone()),
                )
            })
            .collect();
        self.epoch_store
            .assign_shared_object_versions_idempotent(
                self.get_validator().get_object_cache_reader().as_ref(),
                &transactions,
            )
            .unwrap();
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
