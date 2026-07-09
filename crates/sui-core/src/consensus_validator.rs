// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

use std::sync::Arc;

use consensus_core::{TransactionVerifier, ValidationError};
use consensus_types::block::{BlockRef, TransactionIndex};
use fastcrypto_tbls::dkg_v1;
use itertools::Itertools;
use mysten_metrics::monitored_scope;
use nonempty::NonEmpty;
use prometheus::{
    IntCounter, IntCounterVec, Registry, register_int_counter_vec_with_registry,
    register_int_counter_with_registry,
};
use sui_macros::fail_point_arg;
#[cfg(msim)]
use sui_types::base_types::AuthorityName;
use sui_types::{
    error::{SuiError, SuiErrorKind, SuiResult},
    messages_consensus::{ConsensusPosition, ConsensusTransaction, ConsensusTransactionKind},
    transaction::{PlainTransactionWithClaims, TransactionDataAPI},
};
use tap::TapFallible;
use tracing::{debug, info, instrument, warn};

use crate::{
    authority::{AuthorityState, authority_per_epoch_store::AuthorityPerEpochStore},
    checkpoints::CheckpointServiceNotify,
};

/// Validates transactions from consensus and votes on whether to execute the transactions
/// based on their validity and the current state of the authority.
#[derive(Clone)]
pub struct SuiTxValidator {
    authority_state: Arc<AuthorityState>,
    epoch_store: Arc<AuthorityPerEpochStore>,
    checkpoint_service: Arc<dyn CheckpointServiceNotify + Send + Sync>,
    metrics: Arc<SuiTxValidatorMetrics>,
}

impl SuiTxValidator {
    pub fn new(
        authority_state: Arc<AuthorityState>,
        epoch_store: Arc<AuthorityPerEpochStore>,
        checkpoint_service: Arc<dyn CheckpointServiceNotify + Send + Sync>,
        metrics: Arc<SuiTxValidatorMetrics>,
    ) -> Self {
        info!(
            "SuiTxValidator constructed for epoch {}",
            epoch_store.epoch()
        );
        Self {
            authority_state,
            epoch_store,
            checkpoint_service,
            metrics,
        }
    }

    fn validate_transactions(&self, txs: &[ConsensusTransactionKind]) -> Result<(), SuiError> {
        let epoch_store = &self.epoch_store;
        let mut ckpt_messages = Vec::new();
        let mut ckpt_batch = Vec::new();
        for tx in txs.iter() {
            match tx {
                ConsensusTransactionKind::CertifiedTransaction(_) => {
                    return Err(SuiErrorKind::UnexpectedMessage(
                        "CertifiedTransaction cannot be used when preconsensus locking is disabled"
                            .to_string(),
                    )
                    .into());
                }
                ConsensusTransactionKind::CheckpointSignature(_) => {
                    return Err(SuiErrorKind::UnexpectedMessage(
                        "CheckpointSignature V1 is no longer supported".to_string(),
                    )
                    .into());
                }
                ConsensusTransactionKind::CheckpointSignatureV2(signature) => {
                    ckpt_messages.push(signature.as_ref());
                    ckpt_batch.push(&signature.summary);
                }
                ConsensusTransactionKind::RandomnessDkgMessage(_, bytes) => {
                    if bytes.len() > dkg_v1::DKG_MESSAGES_MAX_SIZE {
                        warn!("batch verification error: DKG Message too large");
                        return Err(SuiErrorKind::InvalidDkgMessageSize.into());
                    }
                }
                ConsensusTransactionKind::RandomnessDkgConfirmation(_, bytes) => {
                    if bytes.len() > dkg_v1::DKG_MESSAGES_MAX_SIZE {
                        warn!("batch verification error: DKG Confirmation too large");
                        return Err(SuiErrorKind::InvalidDkgMessageSize.into());
                    }
                }

                ConsensusTransactionKind::CapabilityNotification(_) => {
                    return Err(SuiErrorKind::UnexpectedMessage(
                        "CapabilityNotification V1 is no longer supported".to_string(),
                    )
                    .into());
                }

                ConsensusTransactionKind::RandomnessStateUpdate(_, _) => {
                    return Err(SuiErrorKind::UnexpectedMessage(
                        "RandomnessStateUpdate is no longer supported".to_string(),
                    )
                    .into());
                }

                ConsensusTransactionKind::EndOfPublish(_)
                | ConsensusTransactionKind::NewJWKFetched(_, _, _)
                | ConsensusTransactionKind::CapabilityNotificationV2(_) => {}

                ConsensusTransactionKind::UserTransaction(_) => {
                    return Err(SuiErrorKind::UnexpectedMessage(
                        "ConsensusTransactionKind::UserTransaction cannot be used when address aliases is enabled or preconsensus locking is disabled".to_string(),
                    )
                    .into());
                }

                ConsensusTransactionKind::UserTransactionV2(tx) => {
                    if epoch_store.protocol_config().address_aliases() {
                        let has_aliases = if epoch_store
                            .protocol_config()
                            .fix_checkpoint_signature_mapping()
                        {
                            tx.aliases().is_some()
                        } else {
                            tx.aliases_v1().is_some()
                        };
                        if !has_aliases {
                            return Err(SuiErrorKind::UnexpectedMessage(
                                "ConsensusTransactionKind::UserTransactionV2 must contain an aliases claim".to_string(),
                            )
                            .into());
                        }
                    }

                    if let Some(aliases) = tx.aliases() {
                        let num_sigs = tx.tx().tx_signatures().len();
                        for (sig_idx, _) in aliases.iter() {
                            if (*sig_idx as usize) >= num_sigs {
                                return Err(SuiErrorKind::UnexpectedMessage(format!(
                                    "UserTransactionV2 alias contains out-of-bounds signature index {sig_idx} (transaction has {num_sigs} signatures)",
                                )).into());
                            }
                        }
                    }

                    // TODO(fastpath): move deterministic verifications of user transactions here.
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
                        return Err(SuiErrorKind::UnexpectedMessage(format!(
                            "ExecutionTimeObservation contains too many estimates: {}",
                            obs.estimates.len()
                        ))
                        .into());
                    }
                }
            }
        }

        let ckpt_count = ckpt_batch.len();

        crate::signature_verifier::batch_verify_checkpoints(epoch_store.committee(), &ckpt_batch)
            .tap_err(|e| warn!("batch verification error: {}", e))?;

        // All checkpoint sigs have been verified, forward them to the checkpoint service
        for ckpt in ckpt_messages {
            self.checkpoint_service.notify_checkpoint_signature(ckpt)?;
        }

        self.metrics
            .checkpoint_signatures_verified
            .inc_by(ckpt_count as u64);
        Ok(())
    }

    #[instrument(level = "debug", skip_all, fields(block_ref))]
    fn vote_transactions(
        &self,
        block_ref: &BlockRef,
        txs: Vec<ConsensusTransactionKind>,
    ) -> Vec<TransactionIndex> {
        let epoch_store = &self.epoch_store;
        let mut reject_txn_votes = Vec::new();
        for (i, tx) in txs.into_iter().enumerate() {
            let tx: PlainTransactionWithClaims = match tx {
                ConsensusTransactionKind::UserTransactionV2(tx) => *tx,
                _ => continue,
            };

            let tx_digest = *tx.tx().digest();
            if let Err(error) = self.vote_transaction(epoch_store, tx) {
                debug!(?tx_digest, "Voting to reject transaction: {error}");
                self.metrics
                    .transaction_reject_votes
                    .with_label_values(&[error.to_variant_name()])
                    .inc();
                reject_txn_votes.push(i as TransactionIndex);
                // Cache the rejection vote reason (error) for the transaction
                epoch_store.set_rejection_vote_reason(
                    ConsensusPosition {
                        epoch: epoch_store.epoch(),
                        block: *block_ref,
                        index: i as TransactionIndex,
                    },
                    &error,
                );
            } else {
                debug!(?tx_digest, "Voting to accept transaction");
            }
        }

        reject_txn_votes
    }

    #[instrument(level = "debug", skip_all, err(level = "debug"), fields(tx_digest = ?tx.tx().digest()))]
    fn vote_transaction(
        &self,
        epoch_store: &Arc<AuthorityPerEpochStore>,
        tx: PlainTransactionWithClaims,
    ) -> SuiResult<()> {
        // Extract claims before consuming the transaction
        let aliases_v2 = tx.aliases();
        let aliases_v1 = tx.aliases_v1();
        let claimed_immutable_ids = tx.get_immutable_objects();
        let inner_tx = tx.into_tx();

        // Currently validity_check() and verify_transaction() are not required to be consistent across validators,
        // so they do not run in validate_transactions(). They can run there once we confirm it is safe.
        inner_tx.validity_check(&epoch_store.tx_validity_check_context())?;

        self.authority_state.check_system_overload(
            inner_tx.data(),
            self.authority_state.check_system_overload_at_signing(),
        )?;

        #[allow(unused_mut)]
        let mut fail_point_always_report_aliases_changed = false;
        fail_point_arg!(
            "consensus-validator-always-report-aliases-changed",
            |for_validators: Vec<AuthorityName>| {
                if for_validators.contains(&self.authority_state.name) {
                    // always report aliases changed in simtests
                    fail_point_always_report_aliases_changed = true;
                }
            }
        );

        let verified_tx = epoch_store.verify_transaction_with_current_aliases(inner_tx)?;

        // aliases must have data when address_aliases() is enabled.
        if epoch_store.protocol_config().address_aliases() {
            let aliases_match = if epoch_store
                .protocol_config()
                .fix_checkpoint_signature_mapping()
            {
                // V2 format comparison
                let Some(claimed_v2) = aliases_v2 else {
                    return Err(
                        SuiErrorKind::InvalidRequest("missing address alias claim".into()).into(),
                    );
                };
                *verified_tx.aliases() == claimed_v2
            } else {
                // V1 format comparison: derive V1 from verified_tx and compare
                let Some(claimed_v1) = aliases_v1 else {
                    return Err(
                        SuiErrorKind::InvalidRequest("missing address alias claim".into()).into(),
                    );
                };
                let computed_v1: Vec<_> = verified_tx
                    .tx()
                    .data()
                    .intent_message()
                    .value
                    .required_signers()
                    .into_iter()
                    .zip_eq(verified_tx.aliases().iter().map(|(_, seq)| *seq))
                    .collect();
                let computed_v1 =
                    NonEmpty::from_vec(computed_v1).expect("must have at least one signer");
                computed_v1 == claimed_v1
            };

            if !aliases_match || fail_point_always_report_aliases_changed {
                return Err(SuiErrorKind::AliasesChanged.into());
            }
        }

        let inner_tx = verified_tx.into_tx();
        // Claim verification runs inside handle_vote_transaction against the loaded
        // input objects, unconditionally — an empty claim list claims that no input is
        // immutable and must be verified too, since the claims control post-consensus
        // locking.
        self.authority_state.handle_vote_transaction(
            epoch_store,
            inner_tx,
            Some(&claimed_immutable_ids),
        )?;

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
        block_ref: &BlockRef,
        batch: &[&[u8]],
    ) -> Result<Vec<TransactionIndex>, ValidationError> {
        let _scope = monitored_scope("VerifyAndVoteBatch");

        let txs: Vec<_> = batch
            .iter()
            .map(|tx| tx_kind_from_bytes(tx))
            .collect::<Result<Vec<_>, _>>()?;

        self.validate_transactions(&txs)
            .map_err(|e| ValidationError::InvalidTransaction(e.to_string()))?;

        Ok(self.vote_transactions(block_ref, txs))
    }
}

pub struct SuiTxValidatorMetrics {
    checkpoint_signatures_verified: IntCounter,
    transaction_reject_votes: IntCounterVec,
}

impl SuiTxValidatorMetrics {
    pub fn new(registry: &Registry) -> Arc<Self> {
        Arc::new(Self {
            checkpoint_signatures_verified: register_int_counter_with_registry!(
                "tx_validator_checkpoint_signatures_verified",
                "Number of checkpoint verified in consensus batch verifier",
                registry
            )
            .unwrap(),
            transaction_reject_votes: register_int_counter_vec_with_registry!(
                "tx_validator_transaction_reject_votes",
                "Number of reject transaction votes per reason",
                &["reason"],
                registry
            )
            .unwrap(),
        })
    }
}

#[cfg(test)]
mod tests {
    use std::num::NonZeroUsize;
    use std::sync::Arc;

    use consensus_core::TransactionVerifier as _;
    use consensus_types::block::BlockRef;
    use fastcrypto::traits::KeyPair;
    use sui_config::transaction_deny_config::TransactionDenyConfigBuilder;
    use sui_macros::sim_test;
    use sui_protocol_config::ProtocolConfig;
    use sui_types::crypto::deterministic_random_account_key;
    use sui_types::error::{SuiErrorKind, UserInputError};
    use sui_types::executable_transaction::VerifiedExecutableTransaction;
    use sui_types::messages_checkpoint::{
        CheckpointContents, CheckpointSignatureMessage, CheckpointSummary, SignedCheckpointSummary,
    };
    use sui_types::messages_consensus::ConsensusPosition;
    use sui_types::{
        base_types::{ExecutionDigests, ObjectID},
        crypto::Ed25519SuiSignature,
        effects::TransactionEffectsAPI as _,
        messages_consensus::ConsensusTransaction,
        object::Object,
        signature::GenericSignature,
        transaction::{PlainTransactionWithClaims, Transaction},
    };

    use crate::authority::ExecutionEnv;
    use crate::{
        authority::test_authority_builder::TestAuthorityBuilder,
        checkpoints::CheckpointServiceNoop,
        consensus_adapter::consensus_tests::{
            test_gas_objects, test_user_transaction, test_user_transactions,
        },
        consensus_validator::{SuiTxValidator, SuiTxValidatorMetrics},
    };

    #[sim_test]
    async fn accept_valid_transaction() {
        // Initialize an authority with a (owned) gas object and a shared object.
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
        let transactions = test_user_transactions(&state, shared_object).await;

        let first_transaction = transactions[0].clone();
        let first_transaction_bytes: Vec<u8> =
            bcs::to_bytes(&ConsensusTransaction::new_user_transaction_v2_message(
                &name1,
                first_transaction.into(),
            ))
            .unwrap();

        let metrics = SuiTxValidatorMetrics::new(&Default::default());
        let validator = SuiTxValidator::new(
            state.clone(),
            state.epoch_store_for_testing().clone(),
            Arc::new(CheckpointServiceNoop {}),
            metrics,
        );
        let res = validator.verify_batch(&[&first_transaction_bytes]);
        assert!(res.is_ok(), "{res:?}");

        let transaction_bytes: Vec<_> = transactions
            .clone()
            .into_iter()
            .map(|tx| {
                bcs::to_bytes(&ConsensusTransaction::new_user_transaction_v2_message(
                    &name1,
                    tx.into(),
                ))
                .unwrap()
            })
            .collect();

        let batch: Vec<_> = transaction_bytes.iter().map(|t| t.as_slice()).collect();
        let res_batch = validator.verify_batch(&batch);
        assert!(res_batch.is_ok(), "{res_batch:?}");

        let bogus_transaction_bytes: Vec<_> = transactions
            .into_iter()
            .map(|tx| {
                // Create a transaction with an invalid signature
                let aliases = tx.aliases().clone();
                let mut signed_tx: Transaction = tx.into_tx().into();
                signed_tx.tx_signatures_mut_for_testing()[0] =
                    GenericSignature::Signature(sui_types::crypto::Signature::Ed25519SuiSignature(
                        Ed25519SuiSignature::default(),
                    ));
                let tx_with_claims = PlainTransactionWithClaims::from_aliases(signed_tx, aliases);
                bcs::to_bytes(&ConsensusTransaction::new_user_transaction_v2_message(
                    &name1,
                    tx_with_claims,
                ))
                .unwrap()
            })
            .collect();

        let batch: Vec<_> = bogus_transaction_bytes
            .iter()
            .map(|t| t.as_slice())
            .collect();
        // verify_batch doesn't verify user transaction signatures (that happens in vote_transaction).
        // Use verify_and_vote_batch to test that bogus transactions are rejected during voting.
        let res_batch = validator.verify_and_vote_batch(&BlockRef::MIN, &batch);
        assert!(res_batch.is_ok());
        // All transactions should be in the rejection list since they have invalid signatures
        let rejections = res_batch.unwrap();
        assert_eq!(
            rejections.len(),
            batch.len(),
            "All bogus transactions should be rejected"
        );
    }

    #[tokio::test]
    async fn test_verify_and_vote_batch() {
        // 1 account keypair
        let (sender, keypair) = deterministic_random_account_key();

        // 8 gas objects.
        let gas_objects: Vec<Object> = (0..8)
            .map(|_| Object::with_id_owner_for_testing(ObjectID::random(), sender))
            .collect();

        // 2 owned objects.
        let owned_objects: Vec<Object> = (0..2)
            .map(|_| Object::with_id_owner_for_testing(ObjectID::random(), sender))
            .collect();
        let denied_object = owned_objects[1].clone();

        let mut objects = gas_objects.clone();
        objects.extend(owned_objects.clone());

        let network_config =
            sui_swarm_config::network_config_builder::ConfigBuilder::new_with_temp_dir()
                .committee_size(NonZeroUsize::new(1).unwrap())
                .with_objects(objects.clone())
                .build();

        // Add the 2nd object in the deny list. Once we try to process/vote on the transaction that depends on this object, it will be rejected.
        let transaction_deny_config = TransactionDenyConfigBuilder::new()
            .add_denied_object(denied_object.id())
            .build();
        let state = TestAuthorityBuilder::new()
            .with_network_config(&network_config, 0)
            .with_transaction_deny_config(transaction_deny_config)
            .build()
            .await;

        // Create two user transactions

        // A valid transaction
        let valid_transaction = test_user_transaction(
            &state,
            sender,
            &keypair,
            gas_objects[0].clone(),
            vec![owned_objects[0].clone()],
        )
        .await;

        // An invalid transaction where the input object is denied
        let invalid_transaction = test_user_transaction(
            &state,
            sender,
            &keypair,
            gas_objects[1].clone(),
            vec![denied_object.clone()],
        )
        .await;

        // Now create the vector with the transactions and serialize them.
        let transactions = vec![valid_transaction, invalid_transaction];
        let serialized_transactions: Vec<_> = transactions
            .into_iter()
            .map(|t| {
                bcs::to_bytes(&ConsensusTransaction::new_user_transaction_v2_message(
                    &state.name,
                    t.into(),
                ))
                .unwrap()
            })
            .collect();
        let batch: Vec<_> = serialized_transactions
            .iter()
            .map(|t| t.as_slice())
            .collect();

        let validator = SuiTxValidator::new(
            state.clone(),
            state.epoch_store_for_testing().clone(),
            Arc::new(CheckpointServiceNoop {}),
            SuiTxValidatorMetrics::new(&Default::default()),
        );

        // WHEN
        let rejected_transactions = validator
            .verify_and_vote_batch(&BlockRef::MAX, &batch)
            .unwrap();

        // THEN
        // The 2nd transaction should be rejected
        assert_eq!(rejected_transactions, vec![1]);

        // AND
        // The reject reason should get cached
        let epoch_store = state.load_epoch_store_one_call_per_task();
        let reason = epoch_store
            .get_rejection_vote_reason(ConsensusPosition {
                epoch: state.load_epoch_store_one_call_per_task().epoch(),
                block: BlockRef::MAX,
                index: 1,
            })
            .expect("Rejection vote reason should be set");

        assert_eq!(
            reason,
            SuiErrorKind::UserInputError {
                error: UserInputError::TransactionDenied {
                    error: format!(
                        "Access to input object {:?} is temporarily disabled",
                        denied_object.id()
                    )
                }
            }
        );
    }

    #[sim_test]
    async fn accept_checkpoint_signature_v2() {
        let network_config =
            sui_swarm_config::network_config_builder::ConfigBuilder::new_with_temp_dir().build();

        let state = TestAuthorityBuilder::new()
            .with_network_config(&network_config, 0)
            .build()
            .await;

        let epoch_store = state.load_epoch_store_one_call_per_task();

        // Create a minimal checkpoint summary and sign it with the validator's protocol key
        let checkpoint_summary = CheckpointSummary::new(
            &ProtocolConfig::get_for_max_version_UNSAFE(),
            epoch_store.epoch(),
            0,
            0,
            &CheckpointContents::new_with_digests_only_for_tests([ExecutionDigests::random()]),
            None,
            Default::default(),
            None,
            0,
            Vec::new(),
            Vec::new(),
        );

        let keypair = network_config.validator_configs()[0].protocol_key_pair();
        let authority = keypair.public().into();
        let signed = SignedCheckpointSummary::new(
            epoch_store.epoch(),
            checkpoint_summary,
            keypair,
            authority,
        );
        let message = CheckpointSignatureMessage { summary: signed };

        let tx = ConsensusTransaction::new_checkpoint_signature_message_v2(message);
        let bytes = bcs::to_bytes(&tx).unwrap();

        let validator = SuiTxValidator::new(
            state.clone(),
            state.epoch_store_for_testing().clone(),
            Arc::new(CheckpointServiceNoop {}),
            SuiTxValidatorMetrics::new(&Default::default()),
        );

        let res = validator.verify_batch(&[&bytes]);
        assert!(res.is_ok(), "{res:?}");
    }

    // Claim verification is a pure function of the loaded input objects; existence and
    // version/digest liveness are enforced separately by the deny checks and
    // validate_owned_object_versions in the same handle_vote_transaction call.
    #[test]
    fn test_verify_immutable_object_claims() {
        use crate::authority::verify_immutable_object_claims;
        use sui_types::transaction::{InputObjectKind, InputObjects, ObjectReadResult};

        let (sender, _keypair) = deterministic_random_account_key();

        let owned_object1 = Object::with_id_owner_for_testing(ObjectID::random(), sender);
        let owned_object2 = Object::with_id_owner_for_testing(ObjectID::random(), sender);
        let immutable_object1 = Object::immutable_with_id_for_testing(ObjectID::random());
        let immutable_object2 = Object::immutable_with_id_for_testing(ObjectID::random());

        let owned_id1 = owned_object1.id();
        let immutable_id1 = immutable_object1.id();
        let immutable_id2 = immutable_object2.id();

        let input = |obj: &Object| {
            ObjectReadResult::new(
                InputObjectKind::ImmOrOwnedMoveObject(obj.compute_object_reference()),
                obj.clone().into(),
            )
        };
        let inputs = |objs: &[&Object]| InputObjects::new(objs.iter().map(|o| input(o)).collect());

        // Empty claims with no immutable inputs - should pass
        let result =
            verify_immutable_object_claims(&[], &inputs(&[&owned_object1, &owned_object2]));
        assert!(result.is_ok(), "got error: {:?}", result.err());

        // Correct claim - should pass
        let result = verify_immutable_object_claims(
            &[immutable_id1],
            &inputs(&[&owned_object1, &immutable_object1]),
        );
        assert!(result.is_ok(), "got error: {:?}", result.err());

        // Multiple correct claims - should pass
        let result = verify_immutable_object_claims(
            &[immutable_id1, immutable_id2],
            &inputs(&[&owned_object1, &immutable_object1, &immutable_object2]),
        );
        assert!(result.is_ok(), "got error: {:?}", result.err());

        // A fully stripped claim list with an immutable input must fail: the empty list
        // claims that no input is immutable, and completeness is enforced even then.
        let result =
            verify_immutable_object_claims(&[], &inputs(&[&owned_object1, &immutable_object1]));
        assert!(
            matches!(
                result.unwrap_err().as_inner(),
                SuiErrorKind::ImmutableObjectNotClaimed { object_id }
                if *object_id == immutable_id1
            ),
            "expected ImmutableObjectNotClaimed"
        );

        // Partially stripped claims - should fail on the unclaimed immutable input
        let result = verify_immutable_object_claims(
            &[immutable_id1],
            &inputs(&[&immutable_object1, &immutable_object2]),
        );
        assert!(
            matches!(
                result.unwrap_err().as_inner(),
                SuiErrorKind::ImmutableObjectNotClaimed { object_id }
                if *object_id == immutable_id2
            ),
            "expected ImmutableObjectNotClaimed"
        );

        // False claim on a mutable owned object - should fail
        let result = verify_immutable_object_claims(
            &[owned_id1],
            &inputs(&[&owned_object1, &owned_object2]),
        );
        assert!(
            matches!(
                result.unwrap_err().as_inner(),
                SuiErrorKind::InvalidImmutableObjectClaim { claimed_object_id, .. }
                if *claimed_object_id == owned_id1
            ),
            "expected InvalidImmutableObjectClaim"
        );

        // Claim naming an object that is not among the inputs - should fail
        let result = verify_immutable_object_claims(
            &[immutable_id1],
            &inputs(&[&owned_object1, &owned_object2]),
        );
        assert!(
            matches!(
                result.unwrap_err().as_inner(),
                SuiErrorKind::ImmutableObjectClaimNotFoundInInput { object_id }
                if *object_id == immutable_id1
            ),
            "expected ImmutableObjectClaimNotFoundInInput"
        );
    }

    #[sim_test]
    async fn accept_already_executed_transaction() {
        let (sender, keypair) = deterministic_random_account_key();

        let gas_object = Object::with_id_owner_for_testing(ObjectID::random(), sender);
        let owned_object = Object::with_id_owner_for_testing(ObjectID::random(), sender);

        let network_config =
            sui_swarm_config::network_config_builder::ConfigBuilder::new_with_temp_dir()
                .committee_size(NonZeroUsize::new(1).unwrap())
                .with_objects(vec![gas_object.clone(), owned_object.clone()])
                .build();

        let state = TestAuthorityBuilder::new()
            .with_network_config(&network_config, 0)
            .build()
            .await;

        let epoch_store = state.load_epoch_store_one_call_per_task();

        // Create a transaction and execute it.
        let transaction = test_user_transaction(
            &state,
            sender,
            &keypair,
            gas_object.clone(),
            vec![owned_object.clone()],
        )
        .await;
        let tx_digest = *transaction.tx().digest();
        let cert =
            VerifiedExecutableTransaction::new_from_consensus(transaction.clone().into_tx(), 0);
        let (executed_effects, _) = state
            .try_execute_immediately(&cert, ExecutionEnv::new(), &state.epoch_store_for_testing())
            .unwrap();

        // Verify the transaction is executed.
        let read_effects = state
            .get_transaction_cache_reader()
            .get_executed_effects(&tx_digest)
            .expect("Transaction should be executed");
        assert_eq!(read_effects, executed_effects);
        assert_eq!(read_effects.executed_epoch(), epoch_store.epoch());

        // Now try to vote on the already executed transaction using UserTransactionV2
        let serialized_tx = bcs::to_bytes(&ConsensusTransaction::new_user_transaction_v2_message(
            &state.name,
            transaction.into(),
        ))
        .unwrap();
        let validator = SuiTxValidator::new(
            state.clone(),
            state.epoch_store_for_testing().clone(),
            Arc::new(CheckpointServiceNoop {}),
            SuiTxValidatorMetrics::new(&Default::default()),
        );
        let rejected_transactions = validator
            .verify_and_vote_batch(&BlockRef::MAX, &[&serialized_tx])
            .expect("Verify and vote should succeed");

        // The executed transaction should NOT be rejected.
        assert!(rejected_transactions.is_empty());
    }

    #[tokio::test]
    async fn test_reject_invalid_alias_signature_index() {
        let (sender, keypair) = deterministic_random_account_key();

        let gas_object = Object::with_id_owner_for_testing(ObjectID::random(), sender);
        let owned_object = Object::with_id_owner_for_testing(ObjectID::random(), sender);

        let network_config =
            sui_swarm_config::network_config_builder::ConfigBuilder::new_with_temp_dir()
                .committee_size(NonZeroUsize::new(1).unwrap())
                .with_objects(vec![gas_object.clone(), owned_object.clone()])
                .build();

        let state = TestAuthorityBuilder::new()
            .with_network_config(&network_config, 0)
            .build()
            .await;

        let transaction = test_user_transaction(
            &state,
            sender,
            &keypair,
            gas_object.clone(),
            vec![owned_object.clone()],
        )
        .await;

        // Extract the inner transaction and construct a PlainTransactionWithClaims
        // with a bogus alias where sig_idx = 255 (far exceeding the 1 signature).
        let inner_tx: Transaction = transaction.into_tx().into();
        let bogus_aliases = nonempty::nonempty![(255u8, None)];
        let tx_with_bogus_alias = PlainTransactionWithClaims::from_aliases(inner_tx, bogus_aliases);

        let serialized_tx = bcs::to_bytes(&ConsensusTransaction::new_user_transaction_v2_message(
            &state.name,
            tx_with_bogus_alias,
        ))
        .unwrap();

        let validator = SuiTxValidator::new(
            state.clone(),
            state.epoch_store_for_testing().clone(),
            Arc::new(CheckpointServiceNoop {}),
            SuiTxValidatorMetrics::new(&Default::default()),
        );

        let res = validator.verify_batch(&[&serialized_tx]);
        assert!(
            res.is_err(),
            "Should reject transaction with out-of-bounds alias signature index"
        );
    }
}
