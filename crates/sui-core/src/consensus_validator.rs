// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

use std::{collections::HashSet, sync::Arc};

use consensus_core::{TransactionVerifier, ValidationError};
use consensus_types::block::{BlockRef, TransactionIndex};
use fastcrypto_tbls::dkg_v1;
use mysten_metrics::monitored_scope;
use prometheus::{
    IntCounter, IntCounterVec, Registry, register_int_counter_vec_with_registry,
    register_int_counter_with_registry,
};
use sui_macros::fail_point_arg;
#[cfg(msim)]
use sui_types::base_types::AuthorityName;
use sui_types::{
    base_types::{ObjectID, ObjectRef},
    error::{SuiError, SuiErrorKind, SuiResult, UserInputError},
    messages_consensus::{ConsensusPosition, ConsensusTransaction, ConsensusTransactionKind},
    transaction::{
        InputObjectKind, PlainTransactionWithClaims, TransactionDataAPI, TransactionWithClaims,
    },
};
use tap::TapFallible;
use tracing::{debug, info, instrument, warn};

use crate::{
    authority::{AuthorityState, authority_per_epoch_store::AuthorityPerEpochStore},
    checkpoints::CheckpointServiceNotify,
    consensus_adapter::{ConsensusOverloadChecker, NoopConsensusOverloadChecker},
};

/// Validates transactions from consensus and votes on whether to execute the transactions
/// based on their validity and the current state of the authority.
#[derive(Clone)]
pub struct SuiTxValidator {
    authority_state: Arc<AuthorityState>,
    epoch_store: Arc<AuthorityPerEpochStore>,
    consensus_overload_checker: Arc<dyn ConsensusOverloadChecker>,
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
        // Intentionally do not check consensus overload, because this is validating transactions already in consensus.
        let consensus_overload_checker = Arc::new(NoopConsensusOverloadChecker {});
        Self {
            authority_state,
            epoch_store,
            consensus_overload_checker,
            checkpoint_service,
            metrics,
        }
    }

    fn validate_transactions(&self, txs: &[ConsensusTransactionKind]) -> Result<(), SuiError> {
        let epoch_store = self.epoch_store.clone();
        let mut cert_batch = Vec::new();
        let mut ckpt_messages = Vec::new();
        let mut ckpt_batch = Vec::new();
        for tx in txs.iter() {
            match tx {
                ConsensusTransactionKind::CertifiedTransaction(certificate) => {
                    if epoch_store.protocol_config().disable_preconsensus_locking() {
                        return Err(SuiErrorKind::UnexpectedMessage(
                            "CertifiedTransaction cannot be used when preconsensus locking is disabled".to_string(),
                        )
                        .into());
                    }
                    cert_batch.push(certificate.as_ref());
                }
                ConsensusTransactionKind::CheckpointSignature(signature) => {
                    ckpt_messages.push(signature.as_ref());
                    ckpt_batch.push(&signature.summary);
                }
                ConsensusTransactionKind::CheckpointSignatureV2(signature) => {
                    if !epoch_store
                        .protocol_config()
                        .consensus_checkpoint_signature_key_includes_digest()
                    {
                        return Err(SuiErrorKind::UnexpectedMessage(
                            "ConsensusTransactionKind::CheckpointSignatureV2 is unsupported"
                                .to_string(),
                        )
                        .into());
                    }
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

                ConsensusTransactionKind::CapabilityNotification(_) => {}

                ConsensusTransactionKind::EndOfPublish(_)
                | ConsensusTransactionKind::NewJWKFetched(_, _, _)
                | ConsensusTransactionKind::CapabilityNotificationV2(_)
                | ConsensusTransactionKind::RandomnessStateUpdate(_, _) => {}

                ConsensusTransactionKind::UserTransaction(_) => {
                    if epoch_store.protocol_config().address_aliases()
                        || epoch_store.protocol_config().disable_preconsensus_locking()
                    {
                        return Err(SuiErrorKind::UnexpectedMessage(
                            "ConsensusTransactionKind::UserTransaction cannot be used when address aliases is enabled or preconsensus locking is disabled".to_string(),
                        )
                        .into());
                    }
                }

                ConsensusTransactionKind::UserTransactionV2(tx) => {
                    if !(epoch_store.protocol_config().address_aliases()
                        || epoch_store.protocol_config().disable_preconsensus_locking())
                    {
                        return Err(SuiErrorKind::UnexpectedMessage(
                            "ConsensusTransactionKind::UserTransactionV2 must be used when either address aliases is enabled or preconsensus locking is disabled".to_string(),
                        )
                        .into());
                    }
                    if epoch_store.protocol_config().address_aliases() && tx.aliases().is_none() {
                        return Err(SuiErrorKind::UnexpectedMessage(
                            "ConsensusTransactionKind::UserTransactionV2 must contain an aliases claim".to_string(),
                        )
                        .into());
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

    #[instrument(level = "debug", skip_all, fields(block_ref))]
    fn vote_transactions(
        &self,
        block_ref: &BlockRef,
        txs: Vec<ConsensusTransactionKind>,
    ) -> Vec<TransactionIndex> {
        let epoch_store = self.epoch_store.clone();
        if !epoch_store.protocol_config().mysticeti_fastpath() {
            return vec![];
        }

        let mut reject_txn_votes = Vec::new();
        for (i, tx) in txs.into_iter().enumerate() {
            let tx: PlainTransactionWithClaims = match tx {
                ConsensusTransactionKind::UserTransaction(tx) => {
                    TransactionWithClaims::no_aliases(*tx)
                }
                ConsensusTransactionKind::UserTransactionV2(tx) => *tx,
                _ => continue,
            };

            let tx_digest = *tx.tx().digest();
            if let Err(error) = self.vote_transaction(&epoch_store, tx) {
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
        let aliases = tx.aliases();
        let claimed_immutable_ids = tx.get_immutable_objects();
        let inner_tx = tx.into_tx();

        // Currently validity_check() and verify_transaction() are not required to be consistent across validators,
        // so they do not run in validate_transactions(). They can run there once we confirm it is safe.
        inner_tx.validity_check(&epoch_store.tx_validity_check_context())?;

        self.authority_state.check_system_overload(
            &*self.consensus_overload_checker,
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
        if epoch_store.protocol_config().address_aliases()
            && (*verified_tx.aliases() != aliases.unwrap()
                || fail_point_always_report_aliases_changed)
        {
            return Err(SuiErrorKind::AliasesChanged.into());
        }

        let inner_tx = verified_tx.into_tx();
        self.authority_state
            .handle_vote_transaction(epoch_store, inner_tx.clone())?;

        if epoch_store.protocol_config().disable_preconsensus_locking()
            && !claimed_immutable_ids.is_empty()
        {
            let owned_object_refs: HashSet<ObjectRef> = inner_tx
                .data()
                .transaction_data()
                .input_objects()?
                .iter()
                .filter_map(|obj| match obj {
                    InputObjectKind::ImmOrOwnedMoveObject(obj_ref) => Some(*obj_ref),
                    _ => None,
                })
                .collect();
            self.verify_immutable_object_claims(&claimed_immutable_ids, owned_object_refs)?;
        }

        Ok(())
    }

    /// Verify that all claimed immutable objects are actually immutable.
    /// Rejects if any claimed object doesn't exist locally (can't verify) or is not immutable.
    /// This is stricter than general voting because the claim directly controls locking behavior.
    fn verify_immutable_object_claims(
        &self,
        claimed_ids: &[ObjectID],
        owned_object_refs: HashSet<ObjectRef>,
    ) -> SuiResult<()> {
        if claimed_ids.is_empty() {
            return Ok(());
        }

        let objects = self
            .authority_state
            .get_object_cache_reader()
            .get_objects(claimed_ids);

        for (obj, id) in objects.into_iter().zip(claimed_ids.iter()) {
            match obj {
                Some(o) => {
                    // Object exists but is NOT immutable - invalid claim
                    let object_ref = o.compute_object_reference();
                    if !owned_object_refs.contains(&o.compute_object_reference()) {
                        return Err(SuiErrorKind::ImmutableObjectClaimNotFoundInInput {
                            object_id: *id,
                        }
                        .into());
                    }
                    if !o.is_immutable() {
                        return Err(SuiErrorKind::InvalidImmutableObjectClaim {
                            claimed_object_id: *id,
                            found_object_ref: object_ref,
                        }
                        .into());
                    }
                }
                None => {
                    // Object not found - we can't verify the claim, so we must reject.
                    // This branch should not happen because owned input objects are already validated to exist.
                    return Err(SuiErrorKind::UserInputError {
                        error: UserInputError::ObjectNotFound {
                            object_id: *id,
                            version: None,
                        },
                    }
                    .into());
                }
            }
        }

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
    certificate_signatures_verified: IntCounter,
    checkpoint_signatures_verified: IntCounter,
    transaction_reject_votes: IntCounterVec,
}

impl SuiTxValidatorMetrics {
    pub fn new(registry: &Registry) -> Arc<Self> {
        Arc::new(Self {
            certificate_signatures_verified: register_int_counter_with_registry!(
                "tx_validator_certificate_signatures_verified",
                "Number of certificates verified in consensus batch verifier",
                registry
            )
            .unwrap(),
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
    use sui_protocol_config::{Chain, ProtocolConfig, ProtocolVersion};
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
    async fn reject_checkpoint_signature_v2_when_flag_disabled() {
        // Build a single-validator network and authority with protocol version < 93 (flag disabled)
        let network_config =
            sui_swarm_config::network_config_builder::ConfigBuilder::new_with_temp_dir().build();

        let disabled_cfg =
            ProtocolConfig::get_for_version(ProtocolVersion::new(92), Chain::Unknown);
        let state = TestAuthorityBuilder::new()
            .with_network_config(&network_config, 0)
            .with_protocol_config(disabled_cfg)
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
        assert!(res.is_err());
    }

    #[sim_test]
    async fn accept_checkpoint_signature_v2_when_flag_enabled() {
        // Build a single-validator network and authority with protocol version >= 93 (flag enabled)
        let network_config =
            sui_swarm_config::network_config_builder::ConfigBuilder::new_with_temp_dir().build();

        let enabled_cfg = ProtocolConfig::get_for_version(ProtocolVersion::new(93), Chain::Unknown);
        let state = TestAuthorityBuilder::new()
            .with_network_config(&network_config, 0)
            .with_protocol_config(enabled_cfg)
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

    #[sim_test]
    async fn accept_already_executed_transaction() {
        // This test uses ConsensusTransaction::new_user_transaction_message which creates a
        // UserTransaction. When disable_preconsensus_locking=true (protocol version 105+),
        // UserTransaction is not allowed. Gate with disable_preconsensus_locking=false.
        let _guard = ProtocolConfig::apply_overrides_for_testing(|_, mut config| {
            config.set_disable_preconsensus_locking_for_testing(false);
            config
        });

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
        .await
        .into_tx();
        let tx_digest = *transaction.digest();
        let cert = VerifiedExecutableTransaction::new_from_consensus(transaction.clone(), 0);
        let (executed_effects, _) = state
            .try_execute_immediately(&cert, ExecutionEnv::new(), &state.epoch_store_for_testing())
            .await
            .unwrap();

        // Verify the transaction is executed.
        let read_effects = state
            .get_transaction_cache_reader()
            .get_executed_effects(&tx_digest)
            .expect("Transaction should be executed");
        assert_eq!(read_effects, executed_effects);
        assert_eq!(read_effects.executed_epoch(), epoch_store.epoch());

        // Now try to vote on the already executed transaction
        let serialized_tx = bcs::to_bytes(&ConsensusTransaction::new_user_transaction_message(
            &state.name,
            transaction.into_inner().clone(),
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
}
