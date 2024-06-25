// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

use fastcrypto::hash::MultisetHash;
use fastcrypto::traits::KeyPair;
use futures::future::join_all;
use move_core_types::account_address::AccountAddress;
use move_core_types::ident_str;
use prometheus::Registry;
use shared_crypto::intent::{Intent, IntentScope};
use std::collections::{BTreeMap, HashMap};
use std::path::PathBuf;
use std::sync::Arc;
use std::time::Duration;
use sui_config::genesis::Genesis;
use sui_config::local_ip_utils;
use sui_config::node::AuthorityOverloadConfig;
use sui_framework::BuiltInFramework;
use sui_genesis_builder::validator_info::ValidatorInfo;
use sui_macros::nondeterministic;
use sui_move_build::{BuildConfig, CompiledPackage, SuiPackageHooks};
use sui_protocol_config::ProtocolConfig;
use sui_types::base_types::{random_object_ref, ObjectID};
use sui_types::crypto::{
    generate_proof_of_possession, get_key_pair, AccountKeyPair, AuthorityPublicKeyBytes,
    NetworkKeyPair, SuiKeyPair,
};
use sui_types::crypto::{AuthorityKeyPair, Signer};
use sui_types::effects::{SignedTransactionEffects, TestEffectsBuilder};
use sui_types::error::SuiError;
use sui_types::signature_verification::VerifiedDigestCache;
use sui_types::transaction::ObjectArg;
use sui_types::transaction::{
    CallArg, SignedTransaction, Transaction, TransactionData, TEST_ONLY_GAS_UNIT_FOR_TRANSFER,
};
use sui_types::utils::create_fake_transaction;
use sui_types::utils::to_sender_signed_transaction;
use sui_types::{
    base_types::{AuthorityName, ExecutionDigests, ObjectRef, SuiAddress, TransactionDigest},
    committee::Committee,
    crypto::{AuthoritySignInfo, AuthoritySignature},
    message_envelope::Message,
    object::Object,
    transaction::CertifiedTransaction,
};
use tokio::time::timeout;
use tracing::{info, warn};

use crate::authority::{test_authority_builder::TestAuthorityBuilder, AuthorityState};
use crate::authority_aggregator::{AuthorityAggregator, TimeoutConfig};
use crate::epoch::committee_store::CommitteeStore;
use crate::state_accumulator::StateAccumulator;
use crate::test_authority_clients::LocalAuthorityClient;

const WAIT_FOR_TX_TIMEOUT: Duration = Duration::from_secs(15);

pub async fn send_and_confirm_transaction(
    authority: &AuthorityState,
    fullnode: Option<&AuthorityState>,
    transaction: Transaction,
) -> Result<(CertifiedTransaction, SignedTransactionEffects), SuiError> {
    // Make the initial request
    let epoch_store = authority.load_epoch_store_one_call_per_task();
    let transaction = epoch_store.verify_transaction(transaction)?;
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

    // Submit the confirmation. *Now* execution actually happens, and it should fail when we try to look up our dummy module.
    // we unfortunately don't get a very descriptive error message, but we can at least see that something went wrong inside the VM
    //
    // We also check the incremental effects of the transaction on the live object set against StateAccumulator
    // for testing and regression detection
    let state_acc =
        StateAccumulator::new_for_tests(authority.get_accumulator_store().clone(), &epoch_store);
    let include_wrapped_tombstone = !authority
        .epoch_store_for_testing()
        .protocol_config()
        .simplified_unwrap_then_delete();
    let mut state = state_acc.accumulate_live_object_set(include_wrapped_tombstone);
    let (result, _execution_error_opt) = authority.try_execute_for_test(&certificate).await?;
    let state_after = state_acc.accumulate_live_object_set(include_wrapped_tombstone);
    let effects_acc = state_acc.accumulate_effects(
        vec![result.inner().data().clone()],
        epoch_store.protocol_config(),
    );
    state.union(&effects_acc);

    assert_eq!(state_after.digest(), state.digest());

    if let Some(fullnode) = fullnode {
        fullnode.try_execute_for_test(&certificate).await?;
    }
    Ok((certificate.into_inner(), result.into_inner()))
}

// note: clippy is confused about this being dead - it appears to only be used in cfg(test), but
// adding #[cfg(test)] causes other targets to fail
#[allow(dead_code)]
pub(crate) fn init_state_parameters_from_rng<R>(rng: &mut R) -> (Genesis, AuthorityKeyPair)
where
    R: rand::CryptoRng + rand::RngCore,
{
    let dir = nondeterministic!(tempfile::TempDir::new().unwrap());
    let network_config = sui_swarm_config::network_config_builder::ConfigBuilder::new(&dir)
        .rng(rng)
        .build();
    let genesis = network_config.genesis;
    let authority_key = network_config.validator_configs[0]
        .protocol_key_pair()
        .copy();

    (genesis, authority_key)
}

pub async fn wait_for_tx(digest: TransactionDigest, state: Arc<AuthorityState>) {
    match timeout(
        WAIT_FOR_TX_TIMEOUT,
        state
            .get_transaction_cache_reader()
            .notify_read_executed_effects(&[digest]),
    )
    .await
    {
        Ok(_) => info!(?digest, "digest found"),
        Err(e) => {
            warn!(?digest, "digest not found!");
            panic!("timed out waiting for effects of digest! {e}");
        }
    }
}

pub async fn wait_for_all_txes(digests: Vec<TransactionDigest>, state: Arc<AuthorityState>) {
    match timeout(
        WAIT_FOR_TX_TIMEOUT,
        state
            .get_transaction_cache_reader()
            .notify_read_executed_effects(&digests),
    )
    .await
    {
        Ok(_) => info!(?digests, "all digests found"),
        Err(e) => {
            warn!(?digests, "some digests not found!");
            panic!("timed out waiting for effects of digests! {e}");
        }
    }
}

pub fn create_fake_cert_and_effect_digest<'a>(
    signers: impl Iterator<
        Item = (
            &'a AuthorityName,
            &'a (dyn Signer<AuthoritySignature> + Send + Sync),
        ),
    >,
    committee: &Committee,
) -> (ExecutionDigests, CertifiedTransaction) {
    let transaction = create_fake_transaction();
    let cert = CertifiedTransaction::new(
        transaction.data().clone(),
        signers
            .map(|(name, signer)| {
                AuthoritySignInfo::new(
                    committee.epoch,
                    transaction.data(),
                    Intent::sui_app(IntentScope::SenderSignedTransaction),
                    *name,
                    signer,
                )
            })
            .collect(),
        committee,
    )
    .unwrap();
    let effects = TestEffectsBuilder::new(transaction.data()).build();
    (
        ExecutionDigests::new(*transaction.digest(), effects.digest()),
        cert,
    )
}

pub fn compile_basics_package() -> CompiledPackage {
    compile_example_package("../../sui_programmability/examples/basics")
}

pub fn compile_managed_coin_package() -> CompiledPackage {
    compile_example_package("../../crates/sui-core/src/unit_tests/data/managed_coin")
}

pub fn compile_example_package(relative_path: &str) -> CompiledPackage {
    move_package::package_hooks::register_package_hooks(Box::new(SuiPackageHooks));
    let mut path = PathBuf::from(env!("CARGO_MANIFEST_DIR"));
    path.push(relative_path);

    BuildConfig::new_for_testing().build(&path).unwrap()
}

async fn init_genesis(
    committee_size: usize,
    mut genesis_objects: Vec<Object>,
) -> (
    Genesis,
    Vec<(AuthorityPublicKeyBytes, AuthorityKeyPair)>,
    ObjectID,
) {
    // add object_basics package object to genesis
    let modules: Vec<_> = compile_basics_package().get_modules().cloned().collect();
    let genesis_move_packages: Vec<_> = BuiltInFramework::genesis_move_packages().collect();
    let config = ProtocolConfig::get_for_max_version_UNSAFE();
    let pkg = Object::new_package(
        &modules,
        TransactionDigest::genesis_marker(),
        config.max_move_package_size(),
        config.move_binary_format_version(),
        &genesis_move_packages,
    )
    .unwrap();
    let pkg_id = pkg.id();
    genesis_objects.push(pkg);

    let mut builder = sui_genesis_builder::Builder::new().add_objects(genesis_objects);
    let mut key_pairs = Vec::new();
    for i in 0..committee_size {
        let key_pair: AuthorityKeyPair = get_key_pair().1;
        let authority_name = key_pair.public().into();
        let worker_key_pair: NetworkKeyPair = get_key_pair().1;
        let worker_name = worker_key_pair.public().clone();
        let account_key_pair: SuiKeyPair = get_key_pair::<AccountKeyPair>().1.into();
        let network_key_pair: NetworkKeyPair = get_key_pair().1;
        let validator_info = ValidatorInfo {
            name: format!("validator-{i}"),
            protocol_key: authority_name,
            worker_key: worker_name,
            account_address: SuiAddress::from(&account_key_pair.public()),
            network_key: network_key_pair.public().clone(),
            gas_price: 1,
            commission_rate: 0,
            network_address: local_ip_utils::new_local_tcp_address_for_testing(),
            p2p_address: local_ip_utils::new_local_udp_address_for_testing(),
            narwhal_primary_address: local_ip_utils::new_local_udp_address_for_testing(),
            narwhal_worker_address: local_ip_utils::new_local_udp_address_for_testing(),
            description: String::new(),
            image_url: String::new(),
            project_url: String::new(),
        };
        let pop = generate_proof_of_possession(&key_pair, (&account_key_pair.public()).into());
        builder = builder.add_validator(validator_info, pop);
        key_pairs.push((authority_name, key_pair));
    }
    for (_, key) in &key_pairs {
        builder = builder.add_validator_signature(key);
    }
    let genesis = builder.build();
    (genesis, key_pairs, pkg_id)
}

pub async fn init_local_authorities(
    committee_size: usize,
    genesis_objects: Vec<Object>,
) -> (
    AuthorityAggregator<LocalAuthorityClient>,
    Vec<Arc<AuthorityState>>,
    Genesis,
    ObjectID,
) {
    let (genesis, key_pairs, framework) = init_genesis(committee_size, genesis_objects).await;
    let authorities = join_all(key_pairs.iter().map(|(_, key_pair)| {
        TestAuthorityBuilder::new()
            .with_genesis_and_keypair(&genesis, key_pair)
            .build()
    }))
    .await;
    let aggregator = init_local_authorities_with_genesis(&genesis, authorities.clone()).await;
    (aggregator, authorities, genesis, framework)
}

pub async fn init_local_authorities_with_overload_thresholds(
    committee_size: usize,
    genesis_objects: Vec<Object>,
    overload_thresholds: AuthorityOverloadConfig,
) -> (
    AuthorityAggregator<LocalAuthorityClient>,
    Vec<Arc<AuthorityState>>,
    Genesis,
    ObjectID,
) {
    let (genesis, key_pairs, framework) = init_genesis(committee_size, genesis_objects).await;
    let authorities = join_all(key_pairs.iter().map(|(_, key_pair)| {
        TestAuthorityBuilder::new()
            .with_genesis_and_keypair(&genesis, key_pair)
            .with_authority_overload_config(overload_thresholds.clone())
            .build()
    }))
    .await;
    let aggregator = init_local_authorities_with_genesis(&genesis, authorities.clone()).await;
    (aggregator, authorities, genesis, framework)
}

pub async fn init_local_authorities_with_genesis(
    genesis: &Genesis,
    authorities: Vec<Arc<AuthorityState>>,
) -> AuthorityAggregator<LocalAuthorityClient> {
    telemetry_subscribers::init_for_testing();
    let committee = genesis.committee().unwrap();

    let mut clients = BTreeMap::new();
    for state in authorities {
        let name = state.name;
        let client = LocalAuthorityClient::new_from_authority(state);
        clients.insert(name, client);
    }
    let timeouts = TimeoutConfig {
        pre_quorum_timeout: Duration::from_secs(5),
        post_quorum_timeout: Duration::from_secs(5),
        serial_authority_request_interval: Duration::from_secs(1),
    };
    let committee_store = Arc::new(CommitteeStore::new_for_testing(&committee));
    AuthorityAggregator::new_with_timeouts(
        committee,
        committee_store,
        clients,
        &Registry::new(),
        Arc::new(HashMap::new()),
        timeouts,
    )
}

pub fn make_transfer_sui_transaction(
    gas_object: ObjectRef,
    recipient: SuiAddress,
    amount: Option<u64>,
    sender: SuiAddress,
    keypair: &AccountKeyPair,
    gas_price: u64,
) -> Transaction {
    let data = TransactionData::new_transfer_sui(
        recipient,
        sender,
        amount,
        gas_object,
        gas_price * TEST_ONLY_GAS_UNIT_FOR_TRANSFER,
        gas_price,
    );
    to_sender_signed_transaction(data, keypair)
}

pub fn make_pay_sui_transaction(
    gas_object: ObjectRef,
    coins: Vec<ObjectRef>,
    recipients: Vec<SuiAddress>,
    amounts: Vec<u64>,
    sender: SuiAddress,
    keypair: &AccountKeyPair,
    gas_price: u64,
    gas_budget: u64,
) -> Transaction {
    let data = TransactionData::new_pay_sui(
        sender, coins, recipients, amounts, gas_object, gas_budget, gas_price,
    )
    .unwrap();
    to_sender_signed_transaction(data, keypair)
}

pub fn make_transfer_object_transaction(
    object_ref: ObjectRef,
    gas_object: ObjectRef,
    sender: SuiAddress,
    keypair: &AccountKeyPair,
    recipient: SuiAddress,
    gas_price: u64,
) -> Transaction {
    let data = TransactionData::new_transfer(
        recipient,
        object_ref,
        sender,
        gas_object,
        gas_price * TEST_ONLY_GAS_UNIT_FOR_TRANSFER * 10,
        gas_price,
    );
    to_sender_signed_transaction(data, keypair)
}

pub fn make_transfer_object_move_transaction(
    src: SuiAddress,
    keypair: &AccountKeyPair,
    dest: SuiAddress,
    object_ref: ObjectRef,
    framework_obj_id: ObjectID,
    gas_object_ref: ObjectRef,
    gas_budget_in_units: u64,
    gas_price: u64,
) -> Transaction {
    let args = vec![
        CallArg::Object(ObjectArg::ImmOrOwnedObject(object_ref)),
        CallArg::Pure(bcs::to_bytes(&AccountAddress::from(dest)).unwrap()),
    ];

    to_sender_signed_transaction(
        TransactionData::new_move_call(
            src,
            framework_obj_id,
            ident_str!("object_basics").to_owned(),
            ident_str!("transfer").to_owned(),
            Vec::new(),
            gas_object_ref,
            args,
            gas_budget_in_units * gas_price,
            gas_price,
        )
        .unwrap(),
        keypair,
    )
}

/// Make a dummy tx that uses random object refs.
pub fn make_dummy_tx(
    receiver: SuiAddress,
    sender: SuiAddress,
    sender_sec: &AccountKeyPair,
) -> Transaction {
    Transaction::from_data_and_signer(
        TransactionData::new_transfer(
            receiver,
            random_object_ref(),
            sender,
            random_object_ref(),
            TEST_ONLY_GAS_UNIT_FOR_TRANSFER * 10,
            10,
        ),
        vec![sender_sec],
    )
}

/// Make a cert using an arbitrarily large committee.
pub fn make_cert_with_large_committee(
    committee: &Committee,
    key_pairs: &[AuthorityKeyPair],
    transaction: &Transaction,
) -> CertifiedTransaction {
    // assumes equal weighting.
    let len = committee.voting_rights.len();
    assert_eq!(len, key_pairs.len());
    let count = (len * 2 + 2) / 3;

    let sigs: Vec<_> = key_pairs
        .iter()
        .take(count)
        .map(|key_pair| {
            SignedTransaction::new(
                committee.epoch(),
                transaction.clone().into_data(),
                key_pair,
                AuthorityPublicKeyBytes::from(key_pair.public()),
            )
            .auth_sig()
            .clone()
        })
        .collect();

    let cert = CertifiedTransaction::new(transaction.clone().into_data(), sigs, committee).unwrap();
    cert.verify_signatures_authenticated(
        committee,
        &Default::default(),
        Arc::new(VerifiedDigestCache::new_empty()),
    )
    .unwrap();
    cert
}
