// Copyright (c) 2021, Facebook, Inc. and its affiliates
// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

use crate::checkpoints::CheckpointServiceNoop;
use crate::consensus_handler::SequencedConsensusTransaction;
use fastcrypto::hash::MultisetHash;
use move_core_types::account_address::AccountAddress;
use move_symbol_pool::Symbol;
use sui_move_build::{BuildConfig, CompiledPackage};
use sui_types::crypto::Signature;
use sui_types::move_package::UpgradePolicy;
use sui_types::programmable_transaction_builder::ProgrammableTransactionBuilder;
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
    TestAuthorityBuilder::new()
        .with_genesis_and_keypair(genesis, authority_key)
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

pub fn build_test_modules_with_dep_addr(
    path: PathBuf,
    dep_original_addresses: impl IntoIterator<Item = (&'static str, ObjectID)>,
    dep_ids: impl IntoIterator<Item = (&'static str, ObjectID)>,
) -> CompiledPackage {
    let mut build_config = BuildConfig::new_for_testing();
    for (addr_name, obj_id) in dep_original_addresses {
        build_config
            .config
            .additional_named_addresses
            .insert(addr_name.to_string(), AccountAddress::from(obj_id));
    }
    let mut package = build_config.build(path).unwrap();

    let dep_id_mapping: BTreeMap<_, _> = dep_ids
        .into_iter()
        .map(|(dep_name, obj_id)| (Symbol::from(dep_name), obj_id))
        .collect();

    assert_eq!(
        dep_id_mapping.len(),
        package.dependency_ids.unpublished.len()
    );
    for unpublished_dep in &package.dependency_ids.unpublished {
        let published_id = dep_id_mapping.get(unpublished_dep).unwrap();
        // Make sure we aren't overriding a package
        assert!(package
            .dependency_ids
            .published
            .insert(*unpublished_dep, *published_id)
            .is_none())
    }

    // No unpublished deps
    package.dependency_ids.unpublished.clear();
    package
}

/// Returns the new package's ID and the upgrade cap object ref.
/// `dep_original_addresses` allows us to fill out mappings in the addresses section of the package manifest. These IDs
/// must be the original IDs of names.
/// dep_ids are the IDs of the dependencies of the package, in the latest version (if there were upgrades).
pub async fn publish_package_on_single_authority(
    path: PathBuf,
    sender: SuiAddress,
    sender_key: &dyn Signer<Signature>,
    gas_payment: ObjectRef,
    dep_original_addresses: impl IntoIterator<Item = (&'static str, ObjectID)>,
    dep_ids: Vec<ObjectID>,
    state: &Arc<AuthorityState>,
) -> SuiResult<(ObjectID, ObjectRef)> {
    let mut build_config = BuildConfig::new_for_testing();
    for (addr_name, obj_id) in dep_original_addresses {
        build_config
            .config
            .additional_named_addresses
            .insert(addr_name.to_string(), AccountAddress::from(obj_id));
    }
    let modules = build_config.build(path).unwrap().get_package_bytes(false);

    let mut builder = ProgrammableTransactionBuilder::new();
    let cap = builder.publish_upgradeable(modules, dep_ids);
    builder.transfer_arg(sender, cap);
    let pt = builder.finish();

    let rgp = state.epoch_store_for_testing().reference_gas_price();
    let txn_data = TransactionData::new_programmable(
        sender,
        vec![gas_payment],
        pt,
        rgp * TEST_ONLY_GAS_UNIT_FOR_PUBLISH,
        rgp,
    );

    let signed = to_sender_signed_transaction(txn_data, sender_key);
    let (_cert, effects) = send_and_confirm_transaction(state, signed).await?;
    assert!(effects.data().status().is_ok());
    let package_id = effects
        .data()
        .created()
        .iter()
        .find(|c| c.1 == Owner::Immutable)
        .unwrap()
        .0
         .0;
    let cap_object = effects
        .data()
        .created()
        .iter()
        .find(|c| matches!(c.1, Owner::AddressOwner(..)))
        .unwrap()
        .0;
    Ok((package_id, cap_object))
}

pub async fn upgrade_package_on_single_authority(
    path: PathBuf,
    sender: SuiAddress,
    sender_key: &dyn Signer<Signature>,
    gas_payment: ObjectRef,
    package_id: ObjectID,
    upgrade_cap: ObjectRef,
    dep_original_addresses: impl IntoIterator<Item = (&'static str, ObjectID)>,
    dep_id_mapping: impl IntoIterator<Item = (&'static str, ObjectID)>,
    state: &Arc<AuthorityState>,
) -> SuiResult<ObjectID> {
    let package = build_test_modules_with_dep_addr(path, dep_original_addresses, dep_id_mapping);

    let with_unpublished_deps = false;
    let hash_modules = true;
    let modules = package.get_package_bytes(with_unpublished_deps);
    let digest = package
        .get_package_digest(with_unpublished_deps, hash_modules)
        .to_vec();

    let rgp = state.epoch_store_for_testing().reference_gas_price();
    let data = TransactionData::new_upgrade(
        sender,
        gas_payment,
        package_id,
        modules,
        package.published_dependency_ids(),
        (upgrade_cap, Owner::AddressOwner(sender)),
        UpgradePolicy::COMPATIBLE,
        digest,
        rgp * TEST_ONLY_GAS_UNIT_FOR_PUBLISH,
        rgp,
    )
    .unwrap();
    let signed = to_sender_signed_transaction(data, sender_key);
    let (_cert, effects) = send_and_confirm_transaction(state, signed).await?;
    assert!(effects.data().status().is_ok());
    let package_id = effects
        .data()
        .created()
        .iter()
        .find(|c| c.1 == Owner::Immutable)
        .unwrap()
        .0
         .0;
    Ok(package_id)
}
