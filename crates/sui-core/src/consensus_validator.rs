// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

use std::sync::Arc;

use consensus_core::{TransactionIndex, TransactionVerifier, ValidationError};
use fastcrypto_tbls::dkg_v1;
use mysten_metrics::monitored_scope;
use prometheus::{register_int_counter_with_registry, IntCounter, Registry};
use sui_types::{
    error::{SuiError, SuiResult},
    messages_consensus::{ConsensusTransaction, ConsensusTransactionKind},
    transaction::Transaction,
};
use tap::TapFallible;
use tracing::{debug, info, warn};

use crate::{
    authority::{authority_per_epoch_store::AuthorityPerEpochStore, AuthorityState},
    checkpoints::CheckpointServiceNotify,
    consensus_adapter::ConsensusOverloadChecker,
    transaction_manager::TransactionManager,
};

/// Allows verifying the validity of transactions
#[derive(Clone)]
pub struct SuiTxValidator {
    authority_state: Arc<AuthorityState>,
    consensus_overload_checker: Arc<dyn ConsensusOverloadChecker>,
    checkpoint_service: Arc<dyn CheckpointServiceNotify + Send + Sync>,
    _transaction_manager: Arc<TransactionManager>,
    metrics: Arc<SuiTxValidatorMetrics>,
}

impl SuiTxValidator {
    pub fn new(
        authority_state: Arc<AuthorityState>,
        consensus_overload_checker: Arc<dyn ConsensusOverloadChecker>,
        checkpoint_service: Arc<dyn CheckpointServiceNotify + Send + Sync>,
        transaction_manager: Arc<TransactionManager>,
        metrics: Arc<SuiTxValidatorMetrics>,
    ) -> Self {
        let epoch_store = authority_state.load_epoch_store_one_call_per_task().clone();
        info!(
            "SuiTxValidator constructed for epoch {}",
            epoch_store.epoch()
        );
        Self {
            authority_state,
            consensus_overload_checker,
            checkpoint_service,
            _transaction_manager: transaction_manager,
            metrics,
        }
    }

    fn validate_transactions(&self, txs: &[ConsensusTransactionKind]) -> Result<(), SuiError> {
        let epoch_store = self.authority_state.load_epoch_store_one_call_per_task();

        let mut cert_batch = Vec::new();
        let mut ckpt_messages = Vec::new();
        let mut ckpt_batch = Vec::new();
        for tx in txs.iter() {
            match tx {
                ConsensusTransactionKind::CertifiedTransaction(certificate) => {
                    cert_batch.push(certificate.as_ref());
                }
                ConsensusTransactionKind::CheckpointSignature(signature) => {
                    ckpt_messages.push(signature.as_ref());
                    ckpt_batch.push(&signature.summary);
                }
                ConsensusTransactionKind::RandomnessDkgMessage(_, bytes) => {
                    if bytes.len() > dkg_v1::DKG_MESSAGES_MAX_SIZE {
                        warn!("batch verification error: DKG Message too large");
                        return Err(SuiError::InvalidDkgMessageSize);
                    }
                }
                ConsensusTransactionKind::RandomnessDkgConfirmation(_, bytes) => {
                    if bytes.len() > dkg_v1::DKG_MESSAGES_MAX_SIZE {
                        warn!("batch verification error: DKG Confirmation too large");
                        return Err(SuiError::InvalidDkgMessageSize);
                    }
                }

                ConsensusTransactionKind::CapabilityNotification(_) => {}

                ConsensusTransactionKind::EndOfPublish(_)
                | ConsensusTransactionKind::NewJWKFetched(_, _, _)
                | ConsensusTransactionKind::CapabilityNotificationV2(_)
                | ConsensusTransactionKind::RandomnessStateUpdate(_, _) => {}

                ConsensusTransactionKind::UserTransaction(_tx) => {
                    if !epoch_store.protocol_config().mysticeti_fastpath() {
                        return Err(SuiError::UnexpectedMessage(
                            "ConsensusTransactionKind::UserTransaction is unsupported".to_string(),
                        ));
                    }
                    // TODO(fastpath): move deterministic verifications of user transactions here,
                    // for example validity_check() and verify_transaction().
                }

                ConsensusTransactionKind::ExecutionTimeObservation(obs) => {
                    // TODO: Use a separate limit for this that may truncate shared observations.
                    if obs.estimates.len()
                        > epoch_store
                            .protocol_config()
                            .max_programmable_tx_commands()
                            .try_into()
                            .unwrap()
                    {
                        return Err(SuiError::UnexpectedMessage(format!(
                            "ExecutionTimeObservation contains too many estimates: {}",
                            obs.estimates.len()
                        )));
                    }
                }
            }
        }

        // verify the certificate signatures as a batch
        let cert_count = cert_batch.len();
        let ckpt_count = ckpt_batch.len();

        epoch_store
            .signature_verifier
            .verify_certs_and_checkpoints(cert_batch, ckpt_batch)
            .tap_err(|e| warn!("batch verification error: {}", e))?;

        // All checkpoint sigs have been verified, forward them to the checkpoint service
        for ckpt in ckpt_messages {
            self.checkpoint_service
                .notify_checkpoint_signature(&epoch_store, ckpt)?;
        }

        self.metrics
            .certificate_signatures_verified
            .inc_by(cert_count as u64);
        self.metrics
            .checkpoint_signatures_verified
            .inc_by(ckpt_count as u64);
        Ok(())
    }

    fn vote_transactions(&self, txs: Vec<ConsensusTransactionKind>) -> Vec<TransactionIndex> {
        let epoch_store = self.authority_state.load_epoch_store_one_call_per_task();
        if !epoch_store.protocol_config().mysticeti_fastpath() {
            return vec![];
        }

        let mut result = Vec::new();
        for (i, tx) in txs.into_iter().enumerate() {
            let ConsensusTransactionKind::UserTransaction(tx) = tx else {
                continue;
            };

            if let Err(e) = self.vote_transaction(&epoch_store, tx) {
                debug!("Failed to vote transaction: {:?}", e);
                result.push(i as TransactionIndex);
            }
        }

        result
    }

    fn vote_transaction(
        &self,
        epoch_store: &Arc<AuthorityPerEpochStore>,
        tx: Box<Transaction>,
    ) -> SuiResult<()> {
        // Currently validity_check() and verify_transaction() are not required to be consistent across validators,
        // so they do not run in validate_transactions(). They can run there once we confirm it is safe.
        tx.validity_check(epoch_store.protocol_config(), epoch_store.epoch())?;

        self.authority_state.check_system_overload(
            &*self.consensus_overload_checker,
            tx.data(),
            self.authority_state.check_system_overload_at_signing(),
        )?;

        let tx = epoch_store.verify_transaction(*tx)?;

        self.authority_state
            .handle_vote_transaction(epoch_store, tx)?;

        Ok(())
    }
}

fn tx_kind_from_bytes(tx: &[u8]) -> Result<ConsensusTransactionKind, ValidationError> {
    bcs::from_bytes::<ConsensusTransaction>(tx)
        .map_err(|e| {
            ValidationError::InvalidTransaction(format!(
                "Failed to parse transaction bytes: {:?}",
                e
            ))
        })
        .map(|tx| tx.kind)
}

impl TransactionVerifier for SuiTxValidator {
    fn verify_batch(&self, batch: &[&[u8]]) -> Result<(), ValidationError> {
        let _scope = monitored_scope("ValidateBatch");

        let txs: Vec<_> = batch
            .iter()
            .map(|tx| tx_kind_from_bytes(tx))
            .collect::<Result<Vec<_>, _>>()?;

        self.validate_transactions(&txs)
            .map_err(|e| ValidationError::InvalidTransaction(e.to_string()))
    }

    fn verify_and_vote_batch(
        &self,
        batch: &[&[u8]],
    ) -> Result<Vec<TransactionIndex>, ValidationError> {
        let _scope = monitored_scope("VerifyAndVoteBatch");

        let txs: Vec<_> = batch
            .iter()
            .map(|tx| tx_kind_from_bytes(tx))
            .collect::<Result<Vec<_>, _>>()?;

        self.validate_transactions(&txs)
            .map_err(|e| ValidationError::InvalidTransaction(e.to_string()))?;

        Ok(self.vote_transactions(txs))
    }
}

pub struct SuiTxValidatorMetrics {
    certificate_signatures_verified: IntCounter,
    checkpoint_signatures_verified: IntCounter,
}

impl SuiTxValidatorMetrics {
    pub fn new(registry: &Registry) -> Arc<Self> {
        Arc::new(Self {
            certificate_signatures_verified: register_int_counter_with_registry!(
                "certificate_signatures_verified",
                "Number of certificates verified in consensus batch verifier",
                registry
            )
            .unwrap(),
            checkpoint_signatures_verified: register_int_counter_with_registry!(
                "checkpoint_signatures_verified",
                "Number of checkpoint verified in consensus batch verifier",
                registry
            )
            .unwrap(),
        })
    }
}

#[cfg(test)]
mod tests {
    use std::sync::Arc;

    use consensus_core::TransactionVerifier as _;
    use sui_macros::sim_test;
    use sui_types::{
        crypto::Ed25519SuiSignature, messages_consensus::ConsensusTransaction, object::Object,
        signature::GenericSignature,
    };

    use crate::{
        authority::test_authority_builder::TestAuthorityBuilder,
        checkpoints::CheckpointServiceNoop,
        consensus_adapter::{
            consensus_tests::{test_certificates, test_gas_objects},
            NoopConsensusOverloadChecker,
        },
        consensus_validator::{SuiTxValidator, SuiTxValidatorMetrics},
    };

    #[sim_test]
    async fn accept_valid_transaction() {
        // Initialize an authority with a (owned) gas object and a shared object; then
        // make a test certificate.
        let mut objects = test_gas_objects();
        let shared_object = Object::shared_for_testing();
        objects.push(shared_object.clone());

        let network_config =
            sui_swarm_config::network_config_builder::ConfigBuilder::new_with_temp_dir()
                .with_objects(objects.clone())
                .build();

        let state = TestAuthorityBuilder::new()
            .with_network_config(&network_config, 0)
            .build()
            .await;
        let name1 = state.name;
        let certificates = test_certificates(&state, shared_object).await;

        let first_transaction = certificates[0].clone();
        let first_transaction_bytes: Vec<u8> = bcs::to_bytes(
            &ConsensusTransaction::new_certificate_message(&name1, first_transaction),
        )
        .unwrap();

        let metrics = SuiTxValidatorMetrics::new(&Default::default());
        let validator = SuiTxValidator::new(
            state.clone(),
            Arc::new(NoopConsensusOverloadChecker {}),
            Arc::new(CheckpointServiceNoop {}),
            state.transaction_manager().clone(),
            metrics,
        );
        let res = validator.verify_batch(&[&first_transaction_bytes]);
        assert!(res.is_ok(), "{res:?}");

        let transaction_bytes: Vec<_> = certificates
            .clone()
            .into_iter()
            .map(|cert| {
                bcs::to_bytes(&ConsensusTransaction::new_certificate_message(&name1, cert)).unwrap()
            })
            .collect();

        let batch: Vec<_> = transaction_bytes.iter().map(|t| t.as_slice()).collect();
        let res_batch = validator.verify_batch(&batch);
        assert!(res_batch.is_ok(), "{res_batch:?}");

        let bogus_transaction_bytes: Vec<_> = certificates
            .into_iter()
            .map(|mut cert| {
                // set it to an all-zero user signature
                cert.tx_signatures_mut_for_testing()[0] =
                    GenericSignature::Signature(sui_types::crypto::Signature::Ed25519SuiSignature(
                        Ed25519SuiSignature::default(),
                    ));
                bcs::to_bytes(&ConsensusTransaction::new_certificate_message(&name1, cert)).unwrap()
            })
            .collect();

        let batch: Vec<_> = bogus_transaction_bytes
            .iter()
            .map(|t| t.as_slice())
            .collect();
        let res_batch = validator.verify_batch(&batch);
        assert!(res_batch.is_err());
    }
}
