// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

use eyre::WrapErr;
use mysten_metrics::monitored_scope;
use prometheus::{register_int_counter_with_registry, IntCounter, Registry};
use std::sync::Arc;

use crate::authority::authority_per_epoch_store::AuthorityPerEpochStore;
use crate::transaction_manager::TransactionManager;
use async_trait::async_trait;
use narwhal_types::BatchAPI;
use narwhal_worker::TransactionValidator;
use sui_types::messages_consensus::{ConsensusTransaction, ConsensusTransactionKind};
use tap::TapFallible;
use tokio::runtime::Handle;
use tracing::{info, warn};

/// Allows verifying the validity of transactions
#[derive(Clone)]
pub struct SuiTxValidator {
    epoch_store: Arc<AuthorityPerEpochStore>,
    _transaction_manager: Arc<TransactionManager>,
    metrics: Arc<SuiTxValidatorMetrics>,
}

impl SuiTxValidator {
    pub fn new(
        epoch_store: Arc<AuthorityPerEpochStore>,
        transaction_manager: Arc<TransactionManager>,
        metrics: Arc<SuiTxValidatorMetrics>,
    ) -> Self {
        info!(
            "SuiTxValidator constructed for epoch {}",
            epoch_store.epoch()
        );
        Self {
            epoch_store,
            _transaction_manager: transaction_manager,
            metrics,
        }
    }
}

fn tx_from_bytes(tx: &[u8]) -> Result<ConsensusTransaction, eyre::Report> {
    bcs::from_bytes::<ConsensusTransaction>(tx)
        .wrap_err("Malformed transaction (failed to deserialize)")
}

#[async_trait]
impl TransactionValidator for SuiTxValidator {
    type Error = eyre::Report;

    fn validate(&self, _tx: &[u8]) -> Result<(), Self::Error> {
        // We only accept transactions from local sui instance so no need to re-verify it
        Ok(())
    }

    async fn validate_batch(&self, b: &narwhal_types::Batch) -> Result<(), Self::Error> {
        let _scope = monitored_scope("ValidateBatch");
        let txs = b
            .transactions()
            .iter()
            .map(|tx| tx_from_bytes(tx))
            .collect::<Result<Vec<_>, _>>()?;

        let mut cert_batch = Vec::new();
        let mut ckpt_batch = Vec::new();
        for tx in txs.into_iter() {
            match tx.kind {
                ConsensusTransactionKind::UserTransaction(certificate) => {
                    cert_batch.push(*certificate);

                    // if !certificate.contains_shared_object() {
                    //     // new_unchecked safety: we do not use the certs in this list until all
                    //     // have had their signatures verified.
                    //     owned_tx_certs.push(VerifiedCertificate::new_unchecked(*certificate));
                    // }
                }
                ConsensusTransactionKind::CheckpointSignature(signature) => {
                    ckpt_batch.push(signature.summary)
                }
                ConsensusTransactionKind::EndOfPublish(_)
                | ConsensusTransactionKind::CapabilityNotification(_) => {}
            }
        }

        // verify the certificate signatures as a batch
        let cert_count = cert_batch.len();
        let ckpt_count = ckpt_batch.len();
        let epoch_store = self.epoch_store.clone();
        Handle::current()
            .spawn_blocking(move || {
                epoch_store
                    .signature_verifier
                    .verify_certs_and_checkpoints(cert_batch, ckpt_batch)
                    .tap_err(|e| warn!("batch verification error: {}", e))
                    .wrap_err("Malformed batch (failed to verify)")
            })
            .await??;
        self.metrics
            .certificate_signatures_verified
            .inc_by(cert_count as u64);
        self.metrics
            .checkpoint_signatures_verified
            .inc_by(ckpt_count as u64);
        Ok(())

        // todo - we should un-comment line below once we have a way to revert those transactions at the end of epoch
        // all certificates had valid signatures, schedule them for execution prior to sequencing
        // which is unnecessary for owned object transactions.
        // It is unnecessary to write to pending_certificates table because the certs will be written
        // via Narwhal output.
        // self.transaction_manager
        //     .enqueue_certificates(owned_tx_certs, &self.epoch_store)
        //     .wrap_err("Failed to schedule certificates for execution")
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
                "Number of certificates verified in narwhal batch verifier",
                registry
            )
            .unwrap(),
            checkpoint_signatures_verified: register_int_counter_with_registry!(
                "checkpoint_signatures_verified",
                "Number of checkpoint verified in narwhal batch verifier",
                registry
            )
            .unwrap(),
        })
    }
}

#[cfg(test)]
mod tests {
    use crate::{
        consensus_adapter::consensus_tests::{test_certificates, test_gas_objects},
        consensus_validator::{SuiTxValidator, SuiTxValidatorMetrics},
    };
    use narwhal_types::Batch;
    use narwhal_worker::TransactionValidator;
    use sui_types::signature::GenericSignature;

    use crate::authority::test_authority_builder::TestAuthorityBuilder;
    use sui_macros::sim_test;
    use sui_types::crypto::Ed25519SuiSignature;
    use sui_types::messages_consensus::ConsensusTransaction;
    use sui_types::object::Object;

    #[sim_test]
    async fn accept_valid_transaction() {
        // Initialize an authority with a (owned) gas object and a shared object; then
        // make a test certificate.
        let mut objects = test_gas_objects();
        objects.push(Object::shared_for_testing());

        let network_config = sui_config::builder::ConfigBuilder::new_with_temp_dir()
            .with_objects(objects.clone())
            .build();

        let state = TestAuthorityBuilder::new()
            .with_network_config(&network_config)
            .build()
            .await;
        let name1 = state.name;
        let certificates = test_certificates(&state).await;

        let first_transaction = certificates[0].clone();
        let first_transaction_bytes: Vec<u8> = bcs::to_bytes(
            &ConsensusTransaction::new_certificate_message(&name1, first_transaction),
        )
        .unwrap();

        let metrics = SuiTxValidatorMetrics::new(&Default::default());
        let validator = SuiTxValidator::new(
            state.epoch_store_for_testing().clone(),
            state.transaction_manager().clone(),
            metrics,
        );
        let res = validator.validate(&first_transaction_bytes);
        assert!(res.is_ok(), "{res:?}");

        let transaction_bytes: Vec<_> = certificates
            .clone()
            .into_iter()
            .map(|cert| {
                bcs::to_bytes(&ConsensusTransaction::new_certificate_message(&name1, cert)).unwrap()
            })
            .collect();

        let batch = Batch::new(transaction_bytes);
        let res_batch = validator.validate_batch(&batch).await;
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

        let batch = Batch::new(bogus_transaction_bytes);
        let res_batch = validator.validate_batch(&batch).await;
        assert!(res_batch.is_err());
    }
}
