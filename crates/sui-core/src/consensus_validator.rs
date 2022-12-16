// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

use eyre::WrapErr;
use mysten_metrics::monitored_scope;
use prometheus::{register_int_counter_with_registry, IntCounter, Registry};
use std::sync::Arc;

use narwhal_worker::TransactionValidator;
use sui_types::{
    crypto::{AuthoritySignInfoTrait, VerificationObligation},
    messages::{ConsensusTransaction, ConsensusTransactionKind},
};

use crate::authority::AuthorityState;

/// Allows verifying the validity of transactions
#[derive(Clone)]
pub struct SuiTxValidator {
    // a pointer to the Authority state, mostly in order to get access to consensus
    // todo - change it to AuthorityPerEpochStore to avoid race conditions
    state: Arc<AuthorityState>,
    metrics: Arc<SuiTxValidatorMetrics>,
}

impl SuiTxValidator {
    pub fn new(state: Arc<AuthorityState>, registry: &Registry) -> Self {
        let metrics = SuiTxValidatorMetrics::new(registry);
        let metrics = Arc::new(metrics);
        Self { state, metrics }
    }
}

fn tx_from_bytes(tx: &[u8]) -> Result<ConsensusTransaction, eyre::Report> {
    bincode::deserialize::<ConsensusTransaction>(tx)
        .wrap_err("Malformed transaction (failed to deserialize)")
}

impl TransactionValidator for SuiTxValidator {
    type Error = eyre::Report;

    fn validate(&self, _tx: &[u8]) -> Result<(), Self::Error> {
        // We only accept transactions from local sui instance so no need to re-verify it
        Ok(())
    }

    fn validate_batch(&self, b: &narwhal_types::Batch) -> Result<(), Self::Error> {
        let _scope = monitored_scope("ValidateBatch");
        let txs = b
            .transactions
            .iter()
            .map(|tx| tx_from_bytes(tx))
            .collect::<Result<Vec<_>, _>>()?;
        let epoch_store = self.state.epoch_store();

        let mut obligation = VerificationObligation::default();
        for tx in txs.into_iter() {
            match tx.kind {
                ConsensusTransactionKind::UserTransaction(certificate) => {
                    self.metrics.certificate_signatures_verified.inc();
                    // todo - verify user signature when we pin signature in certificate
                    let idx = obligation.add_message(certificate.data(), certificate.epoch());
                    certificate.auth_sig().add_to_verification_obligation(
                        epoch_store.committee(),
                        &mut obligation,
                        idx,
                    )?;
                }
                ConsensusTransactionKind::CheckpointSignature(signature) => {
                    self.metrics.checkpoint_signatures_verified.inc();
                    let summary = signature.summary.summary;
                    let idx = obligation.add_message(&summary, summary.epoch);
                    signature
                        .summary
                        .auth_signature
                        .add_to_verification_obligation(
                            epoch_store.committee(),
                            &mut obligation,
                            idx,
                        )?;
                }
                ConsensusTransactionKind::EndOfPublish(_) => {}
            }
        }
        // verify the user transaction signatures as a batch
        obligation
            .verify_all()
            .wrap_err("Malformed batch (failed to verify)")
    }
}

pub struct SuiTxValidatorMetrics {
    certificate_signatures_verified: IntCounter,
    checkpoint_signatures_verified: IntCounter,
}

impl SuiTxValidatorMetrics {
    pub fn new(registry: &Registry) -> Self {
        Self {
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
        }
    }
}

#[cfg(test)]
mod tests {
    use std::collections::BTreeMap;

    use fastcrypto::traits::KeyPair;
    use narwhal_types::Batch;
    use narwhal_worker::TransactionValidator;
    use sui_types::{
        base_types::AuthorityName,
        committee::Committee,
        crypto::{get_key_pair, AuthorityKeyPair, AuthorityPublicKeyBytes},
        messages::ConsensusTransaction,
    };

    use crate::{
        authority::authority_tests::init_state_with_objects_and_committee,
        consensus_adapter::consensus_tests::{test_certificates, test_gas_objects},
        consensus_validator::SuiTxValidator,
    };

    use sui_macros::sim_test;
    use sui_types::object::Object;

    #[sim_test]
    async fn accept_valid_transaction() {
        // Initialize an authority with a (owned) gas object and a shared object; then
        // make a test certificate.
        let mut objects = test_gas_objects();
        objects.push(Object::shared_for_testing());

        let mut authorities: BTreeMap<AuthorityPublicKeyBytes, u64> = BTreeMap::new();
        let (_a1, sec1): (_, AuthorityKeyPair) = get_key_pair();
        let (_a2, sec2): (_, AuthorityKeyPair) = get_key_pair();
        let name1: AuthorityName = sec1.public().into();
        let name2: AuthorityName = sec2.public().into();

        authorities.insert(name1, 3);
        authorities.insert(name2, 1);

        let committee = Committee::new(0, authorities.clone()).unwrap();

        let state =
            init_state_with_objects_and_committee(objects, Some((committee.clone(), sec1))).await;
        let certificates = test_certificates(&state).await;

        let first_transaction = certificates[0].clone();
        let first_transaction_bytes: Vec<u8> = bincode::serialize(
            &ConsensusTransaction::new_certificate_message(&name1, first_transaction),
        )
        .unwrap();

        let validator = SuiTxValidator::new(state, &Default::default());
        let res = validator.validate(&first_transaction_bytes);
        assert!(res.is_ok(), "{res:?}");

        let transaction_bytes: Vec<_> = certificates
            .into_iter()
            .map(|cert| {
                bincode::serialize(&ConsensusTransaction::new_certificate_message(&name1, cert))
                    .unwrap()
            })
            .collect();

        let batch = Batch::new(transaction_bytes);
        let res_batch = validator.validate_batch(&batch);
        assert!(res_batch.is_ok(), "{res_batch:?}");
    }
}
