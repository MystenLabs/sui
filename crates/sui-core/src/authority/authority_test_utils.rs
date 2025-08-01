// Copyright (c) 2021, Facebook, Inc. and its affiliates
// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

use crate::checkpoints::CheckpointServiceNoop;
use crate::consensus_handler::SequencedConsensusTransaction;
use crate::execution_scheduler::ExecutionSchedulerAPI;
use core::default::Default;
use fastcrypto::hash::MultisetHash;
use fastcrypto::traits::KeyPair;
use sui_protocol_config::Chain;
use sui_types::base_types::FullObjectRef;
use sui_types::crypto::{AccountKeyPair, AuthorityKeyPair};
use sui_types::messages_consensus::ConsensusTransaction;
use sui_types::utils::to_sender_signed_transaction;

use super::shared_object_version_manager::AssignedTxAndVersions;
use super::test_authority_builder::TestAuthorityBuilder;
use super::*;

pub async fn send_and_confirm_transaction(
    authority: &AuthorityState,
    transaction: Transaction,
) -> Result<(CertifiedTransaction, SignedTransactionEffects), SuiError> {
    send_and_confirm_transaction_(
        authority,
        None, /* no fullnode_key_pair */
        transaction,
        false, /* no shared objects */
    )
    .await
}
pub async fn send_and_confirm_transaction_(
    authority: &AuthorityState,
    fullnode: Option<&AuthorityState>,
    transaction: Transaction,
    with_shared: bool, // transaction includes shared objects
) -> Result<(CertifiedTransaction, SignedTransactionEffects), SuiError> {
    let (txn, effects, _execution_error_opt) = send_and_confirm_transaction_with_execution_error(
        authority,
        fullnode,
        transaction,
        with_shared,
        true,
    )
    .await?;
    Ok((txn, effects))
}

pub async fn certify_transaction(
    authority: &AuthorityState,
    transaction: Transaction,
) -> Result<VerifiedCertificate, SuiError> {
    // Make the initial request
    let epoch_store = authority.load_epoch_store_one_call_per_task();
    // TODO: Move this check to a more appropriate place.
    transaction.validity_check(&epoch_store.tx_validity_check_context())?;
    let transaction = epoch_store.verify_transaction(transaction).unwrap();

    let response = authority
        .handle_transaction(&epoch_store, transaction.clone())
        .await?;
    let vote = response.status.into_signed_for_testing();

    // Collect signatures from a quorum of authorities
    let committee = authority.clone_committee_for_testing();
    let certificate = CertifiedTransaction::new(transaction.into_message(), vec![vote], &committee)
        .unwrap()
        .try_into_verified_for_testing(&committee, &Default::default())
        .unwrap();
    Ok(certificate)
}

pub async fn execute_certificate_with_execution_error(
    authority: &AuthorityState,
    fullnode: Option<&AuthorityState>,
    certificate: VerifiedCertificate,
    with_shared: bool, // transaction includes shared objects
    fake_consensus: bool,
) -> Result<
    (
        CertifiedTransaction,
        SignedTransactionEffects,
        Option<ExecutionError>,
    ),
    SuiError,
> {
    let epoch_store = authority.load_epoch_store_one_call_per_task();
    // We also check the incremental effects of the transaction on the live object set against StateAccumulator
    // for testing and regression detection.
    // We must do this before sending to consensus, otherwise consensus may already
    // lead to transaction execution and state change.
    let state_acc =
        GlobalStateHasher::new_for_tests(authority.get_global_state_hash_store().clone());
    let include_wrapped_tombstone = !authority
        .epoch_store_for_testing()
        .protocol_config()
        .simplified_unwrap_then_delete();
    let mut state =
        state_acc.accumulate_cached_live_object_set_for_testing(include_wrapped_tombstone);

    let assigned_versions = if with_shared {
        if fake_consensus {
            send_consensus(authority, &certificate).await
        } else {
            // Just set object locks directly if send_consensus is not requested.
            let assigned_versions = authority
                .epoch_store_for_testing()
                .assign_shared_object_versions_for_tests(
                    authority.get_object_cache_reader().as_ref(),
                    &vec![VerifiedExecutableTransaction::new_from_certificate(
                        certificate.clone(),
                    )],
                )?;
            assigned_versions
                .into_map()
                .get(&certificate.key())
                .cloned()
                .unwrap()
        }
    } else {
        vec![]
    };

    // Submit the confirmation. *Now* execution actually happens, and it should fail when we try to look up our dummy module.
    // we unfortunately don't get a very descriptive error message, but we can at least see that something went wrong inside the VM
    let (result, execution_error_opt) = authority
        .try_execute_for_test(
            &certificate,
            ExecutionEnv::new().with_assigned_versions(assigned_versions.clone()),
        )
        .await?;
    let state_after =
        state_acc.accumulate_cached_live_object_set_for_testing(include_wrapped_tombstone);
    let effects_acc = state_acc.accumulate_effects(
        &[result.inner().data().clone()],
        epoch_store.protocol_config(),
    );
    state.union(&effects_acc);

    assert_eq!(state_after.digest(), state.digest());

    if let Some(fullnode) = fullnode {
        fullnode
            .try_execute_for_test(
                &certificate,
                ExecutionEnv::new().with_assigned_versions(assigned_versions),
            )
            .await?;
    }
    Ok((
        certificate.into_inner(),
        result.into_inner(),
        execution_error_opt,
    ))
}

pub async fn send_and_confirm_transaction_with_execution_error(
    authority: &AuthorityState,
    fullnode: Option<&AuthorityState>,
    transaction: Transaction,
    with_shared: bool,    // transaction includes shared objects
    fake_consensus: bool, // runs consensus handler if true
) -> Result<
    (
        CertifiedTransaction,
        SignedTransactionEffects,
        Option<ExecutionError>,
    ),
    SuiError,
> {
    let certificate = certify_transaction(authority, transaction).await?;
    execute_certificate_with_execution_error(
        authority,
        fullnode,
        certificate,
        with_shared,
        fake_consensus,
    )
    .await
}

pub async fn init_state_validator_with_fullnode() -> (Arc<AuthorityState>, Arc<AuthorityState>) {
    use sui_types::crypto::get_authority_key_pair;

    let validator = TestAuthorityBuilder::new().build().await;
    let fullnode_key_pair = get_authority_key_pair().1;
    let fullnode = TestAuthorityBuilder::new()
        .with_keypair(&fullnode_key_pair)
        .build()
        .await;
    (validator, fullnode)
}

pub async fn init_state_with_committee(
    genesis: &Genesis,
    authority_key: &AuthorityKeyPair,
) -> Arc<AuthorityState> {
    let mut protocol_config =
        ProtocolConfig::get_for_version(ProtocolVersion::max(), Chain::Unknown);
    protocol_config
        .set_per_object_congestion_control_mode_for_testing(PerObjectCongestionControlMode::None);

    TestAuthorityBuilder::new()
        .with_genesis_and_keypair(genesis, authority_key)
        .with_protocol_config(protocol_config)
        .build()
        .await
}

pub async fn init_state_with_ids<I: IntoIterator<Item = (SuiAddress, ObjectID)>>(
    objects: I,
) -> Arc<AuthorityState> {
    let state = TestAuthorityBuilder::new().build().await;
    for (address, object_id) in objects {
        let obj = Object::with_id_owner_for_testing(object_id, address);
        // TODO: Make this part of genesis initialization instead of explicit insert.
        state.insert_genesis_object(obj).await;
    }
    state
}

pub async fn init_state_with_ids_and_versions<
    I: IntoIterator<Item = (SuiAddress, ObjectID, SequenceNumber)>,
>(
    objects: I,
) -> Arc<AuthorityState> {
    let state = TestAuthorityBuilder::new().build().await;
    for (address, object_id, version) in objects {
        let obj = Object::with_id_owner_version_for_testing(
            object_id,
            version,
            Owner::AddressOwner(address),
        );
        state.insert_genesis_object(obj).await;
    }
    state
}

pub async fn init_state_with_objects<I: IntoIterator<Item = Object>>(
    objects: I,
) -> Arc<AuthorityState> {
    let dir = tempfile::TempDir::new().unwrap();
    let network_config = sui_swarm_config::network_config_builder::ConfigBuilder::new(&dir).build();
    let genesis = network_config.genesis;
    let keypair = network_config.validator_configs[0]
        .protocol_key_pair()
        .copy();
    init_state_with_objects_and_committee(objects, &genesis, &keypair).await
}

pub async fn init_state_with_objects_and_committee<I: IntoIterator<Item = Object>>(
    objects: I,
    genesis: &Genesis,
    authority_key: &AuthorityKeyPair,
) -> Arc<AuthorityState> {
    let state = init_state_with_committee(genesis, authority_key).await;
    for o in objects {
        state.insert_genesis_object(o).await;
    }
    state
}

pub async fn init_state_with_object_id(
    address: SuiAddress,
    object: ObjectID,
) -> Arc<AuthorityState> {
    init_state_with_ids(std::iter::once((address, object))).await
}

pub async fn init_state_with_ids_and_expensive_checks<
    I: IntoIterator<Item = (SuiAddress, ObjectID)>,
>(
    objects: I,
    config: ExpensiveSafetyCheckConfig,
) -> Arc<AuthorityState> {
    let state = TestAuthorityBuilder::new()
        .with_expensive_safety_checks(config)
        .build()
        .await;
    for (address, object_id) in objects {
        let obj = Object::with_id_owner_for_testing(object_id, address);
        // TODO: Make this part of genesis initialization instead of explicit insert.
        state.insert_genesis_object(obj).await;
    }
    state
}

pub fn init_transfer_transaction(
    authority_state: &AuthorityState,
    sender: SuiAddress,
    secret: &AccountKeyPair,
    recipient: SuiAddress,
    object_ref: ObjectRef,
    gas_object_ref: ObjectRef,
    gas_budget: u64,
    gas_price: u64,
) -> VerifiedTransaction {
    let data = TransactionData::new_transfer(
        recipient,
        FullObjectRef::from_fastpath_ref(object_ref),
        sender,
        gas_object_ref,
        gas_budget,
        gas_price,
    );
    let tx = to_sender_signed_transaction(data, secret);
    authority_state
        .epoch_store_for_testing()
        .verify_transaction(tx)
        .unwrap()
}

pub fn init_certified_transfer_transaction(
    sender: SuiAddress,
    secret: &AccountKeyPair,
    recipient: SuiAddress,
    object_ref: ObjectRef,
    gas_object_ref: ObjectRef,
    authority_state: &AuthorityState,
) -> VerifiedCertificate {
    let rgp = authority_state.reference_gas_price_for_testing().unwrap();
    let transfer_transaction = init_transfer_transaction(
        authority_state,
        sender,
        secret,
        recipient,
        object_ref,
        gas_object_ref,
        rgp * TEST_ONLY_GAS_UNIT_FOR_TRANSFER,
        rgp,
    );
    init_certified_transaction(transfer_transaction.into(), authority_state)
}

pub fn init_certified_transaction(
    transaction: Transaction,
    authority_state: &AuthorityState,
) -> VerifiedCertificate {
    let epoch_store = authority_state.epoch_store_for_testing();
    let transaction = epoch_store.verify_transaction(transaction).unwrap();

    let vote = VerifiedSignedTransaction::new(
        0,
        transaction.clone(),
        authority_state.name,
        &*authority_state.secret,
    );
    CertifiedTransaction::new(
        transaction.into_message(),
        vec![vote.auth_sig().clone()],
        epoch_store.committee(),
    )
    .unwrap()
    .try_into_verified_for_testing(epoch_store.committee(), &Default::default())
    .unwrap()
}

pub async fn certify_shared_obj_transaction_no_execution(
    authority: &AuthorityState,
    transaction: Transaction,
) -> Result<(VerifiedCertificate, AssignedVersions), SuiError> {
    let epoch_store = authority.load_epoch_store_one_call_per_task();
    let transaction = epoch_store.verify_transaction(transaction).unwrap();
    let response = authority
        .handle_transaction(&epoch_store, transaction.clone())
        .await?;
    let vote = response.status.into_signed_for_testing();

    // Collect signatures from a quorum of authorities
    let committee = authority.clone_committee_for_testing();
    let certificate =
        CertifiedTransaction::new(transaction.into_message(), vec![vote.clone()], &committee)
            .unwrap()
            .try_into_verified_for_testing(&committee, &Default::default())
            .unwrap();

    let assigned_versions = send_consensus_no_execution(authority, &certificate).await;

    Ok((certificate, assigned_versions))
}

pub async fn enqueue_all_and_execute_all(
    authority: &AuthorityState,
    certificates: Vec<(VerifiedCertificate, ExecutionEnv)>,
) -> Result<Vec<TransactionEffects>, SuiError> {
    authority.execution_scheduler.enqueue(
        certificates
            .iter()
            .map(|(cert, env)| {
                (
                    VerifiedExecutableTransaction::new_from_certificate(cert.clone()).into(),
                    env.clone(),
                )
            })
            .collect(),
        &authority.epoch_store_for_testing(),
    );
    let mut output = Vec::new();
    for (cert, _) in certificates {
        let effects = authority.notify_read_effects("", *cert.digest()).await?;
        output.push(effects);
    }
    Ok(output)
}

pub async fn execute_sequenced_certificate_to_effects(
    authority: &AuthorityState,
    certificate: VerifiedCertificate,
    assigned_versions: AssignedVersions,
) -> Result<(TransactionEffects, Option<ExecutionError>), SuiError> {
    let env = ExecutionEnv::new().with_assigned_versions(assigned_versions);
    authority.execution_scheduler.enqueue(
        vec![(
            VerifiedExecutableTransaction::new_from_certificate(certificate.clone()).into(),
            env.clone(),
        )],
        &authority.epoch_store_for_testing(),
    );

    let (result, execution_error_opt) = authority.try_execute_for_test(&certificate, env).await?;
    let effects = result.inner().data().clone();
    Ok((effects, execution_error_opt))
}

pub async fn send_consensus(
    authority: &AuthorityState,
    cert: &VerifiedCertificate,
) -> AssignedVersions {
    let transaction = SequencedConsensusTransaction::new_test(
        ConsensusTransaction::new_certificate_message(&authority.name, cert.clone().into_inner()),
    );

    let (_, assigned_versions) = authority
        .epoch_store_for_testing()
        .process_consensus_transactions_for_tests(
            vec![transaction],
            &Arc::new(CheckpointServiceNoop {}),
            authority.get_object_cache_reader().as_ref(),
            &authority.metrics,
            true,
        )
        .await
        .unwrap();

    let assigned_versions = assigned_versions
        .0
        .into_iter()
        .next()
        .map(|(_, v)| v)
        .unwrap_or(vec![]);

    let certs = vec![(
        VerifiedExecutableTransaction::new_from_certificate(cert.clone()),
        ExecutionEnv::new().with_assigned_versions(assigned_versions.clone()),
    )];

    authority
        .execution_scheduler()
        .enqueue_transactions(certs, &authority.epoch_store_for_testing());

    assigned_versions
}

pub async fn send_consensus_no_execution(
    authority: &AuthorityState,
    cert: &VerifiedCertificate,
) -> AssignedVersions {
    let transaction = SequencedConsensusTransaction::new_test(
        ConsensusTransaction::new_certificate_message(&authority.name, cert.clone().into_inner()),
    );

    // Call process_consensus_transaction() instead of handle_consensus_transaction(), to avoid actually executing cert.
    // This allows testing cert execution independently.
    let (_, assigned_versions) = authority
        .epoch_store_for_testing()
        .process_consensus_transactions_for_tests(
            vec![transaction],
            &Arc::new(CheckpointServiceNoop {}),
            authority.get_object_cache_reader().as_ref(),
            &authority.metrics,
            true,
        )
        .await
        .unwrap();
    assert_eq!(assigned_versions.0.len(), 1);
    assigned_versions.0.into_iter().next().unwrap().1
}

pub async fn send_batch_consensus_no_execution(
    authority: &AuthorityState,
    certificates: &[VerifiedCertificate],
    skip_consensus_commit_prologue_in_test: bool,
) -> (Vec<Schedulable>, AssignedTxAndVersions) {
    let transactions = certificates
        .iter()
        .map(|cert| {
            SequencedConsensusTransaction::new_test(ConsensusTransaction::new_certificate_message(
                &authority.name,
                cert.clone().into_inner(),
            ))
        })
        .collect();

    // Call process_consensus_transaction() instead of handle_consensus_transaction(), to avoid actually executing cert.
    // This allows testing cert execution independently.
    authority
        .epoch_store_for_testing()
        .process_consensus_transactions_for_tests(
            transactions,
            &Arc::new(CheckpointServiceNoop {}),
            authority.get_object_cache_reader().as_ref(),
            &authority.metrics,
            skip_consensus_commit_prologue_in_test,
        )
        .await
        .unwrap()
}
