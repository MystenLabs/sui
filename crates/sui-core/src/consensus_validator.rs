// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

use eyre::WrapErr;
use std::sync::Arc;
use tap::TapFallible;

use narwhal_worker::TransactionValidator;
use sui_types::{
    crypto::{AuthoritySignInfoTrait, VerificationObligation},
    message_envelope::Message,
    messages::{ConsensusTransaction, ConsensusTransactionKind},
};
use tracing::warn;

use crate::authority::AuthorityState;

/// Allows verifying the validity of transactions
#[derive(Clone)]
struct ConsensusTxValidator {
    // a pointer to the Authority state, mostly in order to get access to consensus
    state: Arc<AuthorityState>,
}

impl ConsensusTxValidator {
    #[allow(dead_code)]
    pub fn new(state: Arc<AuthorityState>) -> Self {
        Self { state }
    }
}

fn tx_from_bytes(tx: &[u8]) -> Result<ConsensusTransaction, eyre::Report> {
    bincode::deserialize::<ConsensusTransaction>(tx)
        .wrap_err("Malformed transaction (failed to deserialize)")
}

impl TransactionValidator for ConsensusTxValidator {
    type Error = eyre::Report;

    fn validate(&self, tx: &[u8]) -> Result<(), Self::Error> {
        let transaction = tx_from_bytes(tx)?;
        let committee = self.state.committee.load();
        match &transaction.kind {
            ConsensusTransactionKind::UserTransaction(certificate) => {
                certificate.verify_signature(&committee).tap_err(|err| {
                    warn!("Malformed transaction (failed to verify): {err}");
                })?;
            }
            ConsensusTransactionKind::CheckpointSignature(signature) => {
                signature.verify(&committee).tap_err(|err| {
                    warn!("Ignoring malformed signature (failed to verify): {err}");
                })?;
            }
            ConsensusTransactionKind::EndOfPublish(_) => {}
        }
        Ok(())
    }

    fn validate_batch(&self, b: &narwhal_types::Batch) -> Result<(), Self::Error> {
        let txs = b
            .transactions
            .iter()
            .map(|tx| tx_from_bytes(tx))
            .collect::<Result<Vec<_>, _>>()?;
        let committee = self.state.committee.load();

        let mut obligation = VerificationObligation::default();
        for tx in txs.into_iter() {
            match tx.kind {
                ConsensusTransactionKind::UserTransaction(certificate) => {
                    // verify the inner user signature
                    certificate.data().verify()?;

                    let idx = obligation.add_message(certificate.data(), certificate.epoch());
                    certificate.auth_sig().add_to_verification_obligation(
                        &committee,
                        &mut obligation,
                        idx,
                    )?;
                }
                ConsensusTransactionKind::CheckpointSignature(signature) => {
                    let summary = signature.summary.summary;
                    let idx = obligation.add_message(&summary, summary.epoch);
                    signature
                        .summary
                        .auth_signature
                        .add_to_verification_obligation(&committee, &mut obligation, idx)?;
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

#[cfg(test)]
mod tests {
    use std::{collections::BTreeMap, sync::Arc};

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
        consensus_adapter::consensus_tests::{
            test_certificates, test_gas_objects, test_shared_object,
        },
        consensus_validator::ConsensusTxValidator,
    };

    #[tokio::test]
    async fn accept_valid_transaction() {
        // Initialize an authority with a (owned) gas object and a shared object; then
        // make a test certificate.
        let mut objects = test_gas_objects();
        objects.push(test_shared_object());

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

        let validator = ConsensusTxValidator::new(Arc::new(state));
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
