// Copyright (c) 2021, Facebook, Inc. and its affiliates
// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

use crate::checkpoints::CheckpointServiceNoop;
use crate::consensus_handler::SequencedConsensusTransaction;
use fastcrypto::hash::MultisetHash;
use sui_types::utils::to_sender_signed_transaction;
use sui_types::{
    crypto::{AccountKeyPair, AuthorityKeyPair, KeypairTraits},
    messages::VerifiedTransaction,
};

use super::test_authority_builder::TestAuthorityBuilder;
use super::*;

pub async fn send_and_confirm_transaction(
    authority: &AuthorityState,
    transaction: VerifiedTransaction,
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
    transaction: VerifiedTransaction,
    with_shared: bool, // transaction includes shared objects
) -> Result<(CertifiedTransaction, SignedTransactionEffects), SuiError> {
    let (txn, effects, _execution_error_opt) = send_and_confirm_transaction_with_execution_error(
        authority,
        fullnode,
        transaction,
        with_shared,
    )
    .await?;
    Ok((txn, effects))
}

pub async fn send_and_confirm_transaction_with_execution_error(
    authority: &AuthorityState,
    fullnode: Option<&AuthorityState>,
    transaction: VerifiedTransaction,
    with_shared: bool, // transaction includes shared objects
) -> Result<
    (
        CertifiedTransaction,
        SignedTransactionEffects,
        Option<ExecutionError>,
    ),
    SuiError,
> {
    // Make the initial request
    let epoch_store = authority.load_epoch_store_one_call_per_task();
    let response = authority
        .handle_transaction(&epoch_store, transaction.clone())
        .await?;
    let vote = response.status.into_signed_for_testing();

    // Collect signatures from a quorum of authorities
    let committee = authority.clone_committee_for_testing();
    let certificate =
        CertifiedTransaction::new(transaction.into_message(), vec![vote.clone()], &committee)
            .unwrap()
            .verify(&committee)
            .unwrap();

    if with_shared {
        send_consensus(authority, &certificate).await;
    }

    // Submit the confirmation. *Now* execution actually happens, and it should fail when we try to look up our dummy module.
    // we unfortunately don't get a very descriptive error message, but we can at least see that something went wrong inside the VM
    //
    // We also check the incremental effects of the transaction on the live object set against StateAccumulator
    // for testing and regression detection
    let state_acc = StateAccumulator::new(authority.database.clone());
    let mut state = state_acc.accumulate_live_object_set();
    let (result, execution_error_opt) = authority.try_execute_for_test(&certificate).await?;
    let state_after = state_acc.accumulate_live_object_set();
    let effects_acc = state_acc.accumulate_effects(
        vec![result.inner().data().clone()],
        epoch_store.protocol_config(),
    );
    state.union(&effects_acc);

    assert_eq!(state_after.digest(), state.digest());

    if let Some(fullnode) = fullnode {
        fullnode.try_execute_for_test(&certificate).await?;
    }
    Ok((
        certificate.into_inner(),
        result.into_inner(),
        execution_error_opt,
    ))
}

pub async fn init_state() -> Arc<AuthorityState> {
    let dir = tempfile::TempDir::new().unwrap();
    let network_config = sui_config::builder::ConfigBuilder::new(&dir).build();
    let genesis = network_config.genesis;
    let keypair = network_config.validator_configs[0]
        .protocol_key_pair()
        .copy();

    init_state_with_committee(&genesis, &keypair).await
}

pub async fn init_state_validator_with_fullnode() -> (Arc<AuthorityState>, Arc<AuthorityState>) {
    use sui_types::crypto::get_authority_key_pair;

    let dir = tempfile::TempDir::new().unwrap();
    let network_config = sui_config::builder::ConfigBuilder::new(&dir).build();
    let genesis = network_config.genesis;
    let keypair = network_config.validator_configs[0]
        .protocol_key_pair()
        .copy();

    let validator = init_state_with_committee(&genesis, &keypair).await;
    let fullnode_key_pair = get_authority_key_pair().1;
    let fullnode = init_state_with_committee(&genesis, &fullnode_key_pair).await;
    (validator, fullnode)
}

pub async fn init_state_with_committee(
    genesis: &Genesis,
    authority_key: &AuthorityKeyPair,
) -> Arc<AuthorityState> {
    TestAuthorityBuilder::new()
        .build(genesis.committee().unwrap(), authority_key, genesis)
        .await
}

pub async fn init_state_with_ids<I: IntoIterator<Item = (SuiAddress, ObjectID)>>(
    objects: I,
) -> Arc<AuthorityState> {
    let state = init_state().await;
    for (address, object_id) in objects {
        let obj = Object::with_id_owner_for_testing(object_id, address);
        state.insert_genesis_object(obj).await;
    }
    state
}

pub async fn init_state_with_ids_and_versions<
    I: IntoIterator<Item = (SuiAddress, ObjectID, SequenceNumber)>,
>(
    objects: I,
) -> Arc<AuthorityState> {
    let state = init_state().await;
    for (address, object_id, version) in objects {
        let obj = Object::with_id_owner_version_for_testing(object_id, version, address);
        state.insert_genesis_object(obj).await;
    }
    state
}

pub async fn init_state_with_objects<I: IntoIterator<Item = Object>>(
    objects: I,
) -> Arc<AuthorityState> {
    let dir = tempfile::TempDir::new().unwrap();
    let network_config = sui_config::builder::ConfigBuilder::new(&dir).build();
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

pub fn init_transfer_transaction(
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
        object_ref,
        sender,
        gas_object_ref,
        gas_budget,
        gas_price,
    );
    to_sender_signed_transaction(data, secret)
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
        sender,
        secret,
        recipient,
        object_ref,
        gas_object_ref,
        rgp * TEST_ONLY_GAS_UNIT_FOR_TRANSFER,
        rgp,
    );
    init_certified_transaction(transfer_transaction, authority_state)
}

pub fn init_certified_transaction(
    transaction: VerifiedTransaction,
    authority_state: &AuthorityState,
) -> VerifiedCertificate {
    let vote = VerifiedSignedTransaction::new(
        0,
        transaction.clone(),
        authority_state.name,
        &*authority_state.secret,
    );
    let epoch_store = authority_state.epoch_store_for_testing();
    CertifiedTransaction::new(
        transaction.into_message(),
        vec![vote.auth_sig().clone()],
        epoch_store.committee(),
    )
    .unwrap()
    .verify(epoch_store.committee())
    .unwrap()
}

pub async fn send_consensus(authority: &AuthorityState, cert: &VerifiedCertificate) {
    let transaction = SequencedConsensusTransaction::new_test(
        ConsensusTransaction::new_certificate_message(&authority.name, cert.clone().into_inner()),
    );

    if let Ok(transaction) = authority
        .epoch_store_for_testing()
        .verify_consensus_transaction(transaction, &authority.metrics.skipped_consensus_txns)
    {
        let certs = authority
            .epoch_store_for_testing()
            .process_consensus_transactions(
                vec![transaction],
                &Arc::new(CheckpointServiceNoop {}),
                authority.db(),
            )
            .await
            .unwrap();

        authority
            .transaction_manager()
            .enqueue(certs, &authority.epoch_store_for_testing())
            .unwrap();
    } else {
        warn!("Failed to verify certificate: {:?}", cert);
    }
}

pub async fn send_consensus_no_execution(authority: &AuthorityState, cert: &VerifiedCertificate) {
    let transaction = SequencedConsensusTransaction::new_test(
        ConsensusTransaction::new_certificate_message(&authority.name, cert.clone().into_inner()),
    );

    if let Ok(transaction) = authority
        .epoch_store_for_testing()
        .verify_consensus_transaction(transaction, &authority.metrics.skipped_consensus_txns)
    {
        // Call process_consensus_transaction() instead of handle_consensus_transaction(), to avoid actually executing cert.
        // This allows testing cert execution independently.
        authority
            .epoch_store_for_testing()
            .process_consensus_transactions(
                vec![transaction],
                &Arc::new(CheckpointServiceNoop {}),
                &authority.db(),
            )
            .await
            .unwrap();
    } else {
        warn!("Failed to verify certificate: {:?}", cert);
    }
}
