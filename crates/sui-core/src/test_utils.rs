// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

use crate::authority::{AuthorityState, EffectsNotifyRead};
use crate::authority_aggregator::{AuthorityAggregator, TimeoutConfig};
use crate::epoch::committee_store::CommitteeStore;
use crate::test_authority_clients::LocalAuthorityClient;
use fastcrypto::traits::KeyPair;
use move_core_types::account_address::AccountAddress;
use move_core_types::ident_str;
use prometheus::Registry;
use shared_crypto::intent::{Intent, IntentScope};
use std::collections::BTreeMap;
use std::path::PathBuf;
use std::sync::Arc;
use std::time::Duration;
use sui_config::genesis::Genesis;
use sui_config::ValidatorInfo;
use sui_framework::BuiltInFramework;
use sui_framework_build::compiled_package::{BuildConfig, CompiledPackage, SuiPackageHooks};
use sui_protocol_config::ProtocolConfig;
use sui_types::base_types::{random_object_ref, ObjectID};
use sui_types::crypto::{
    generate_proof_of_possession, get_key_pair, AccountKeyPair, AuthorityPublicKeyBytes,
    NetworkKeyPair, SuiKeyPair,
};
use sui_types::crypto::{AuthorityKeyPair, Signer};
use sui_types::messages::ObjectArg;
use sui_types::messages::TEST_ONLY_GAS_UNIT_FOR_OBJECT_BASICS;
use sui_types::messages::{
    CallArg, SignedTransaction, TransactionData, VerifiedTransaction,
    TEST_ONLY_GAS_UNIT_FOR_TRANSFER,
};
use sui_types::utils::create_fake_transaction;
use sui_types::utils::to_sender_signed_transaction;
use sui_types::{
    base_types::{AuthorityName, ExecutionDigests, ObjectRef, SuiAddress, TransactionDigest},
    committee::Committee,
    crypto::{AuthoritySignInfo, AuthoritySignature},
    message_envelope::Message,
    messages::{CertifiedTransaction, Transaction, TransactionEffects},
    object::Object,
};
use tokio::time::timeout;
use tracing::{info, warn};

const WAIT_FOR_TX_TIMEOUT: Duration = Duration::from_secs(15);

// note: clippy is confused about this being dead - it appears to only be used in cfg(test), but
// adding #[cfg(test)] causes other targets to fail
#[allow(dead_code)]
pub(crate) fn init_state_parameters_from_rng<R>(rng: &mut R) -> (Genesis, AuthorityKeyPair)
where
    R: rand::CryptoRng + rand::RngCore,
{
    let dir = tempfile::TempDir::new().unwrap();
    let network_config = sui_config::builder::ConfigBuilder::new(&dir)
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
        state.database.notify_read_executed_effects(vec![digest]),
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
        state.database.notify_read_executed_effects(digests.clone()),
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
    let effects = dummy_transaction_effects(&transaction);
    (
        ExecutionDigests::new(*transaction.digest(), effects.digest()),
        cert,
    )
}

pub fn dummy_transaction_effects(tx: &Transaction) -> TransactionEffects {
    TransactionEffects::new_with_tx(tx)
}

pub fn compile_basics_package() -> CompiledPackage {
    compile_example_package("../../sui_programmability/examples/basics")
}

pub fn compile_nfts_package() -> CompiledPackage {
    compile_example_package("../../sui_programmability/examples/nfts")
}

pub fn compile_example_package(relative_path: &str) -> CompiledPackage {
    move_package::package_hooks::register_package_hooks(Box::new(SuiPackageHooks));
    let mut path = PathBuf::from(env!("CARGO_MANIFEST_DIR"));
    path.push(relative_path);

    let build_config = BuildConfig::new_for_testing();
    sui_framework::build_move_package(&path, build_config).unwrap()
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
    let pkg = Object::new_package(
        &modules,
        TransactionDigest::genesis(),
        ProtocolConfig::get_for_max_version().max_move_package_size(),
        &genesis_move_packages,
    )
    .unwrap();
    let pkg_id = pkg.id();
    genesis_objects.push(pkg);

    let mut builder = sui_config::genesis::Builder::new().add_objects(genesis_objects);
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
            network_address: sui_config::utils::new_tcp_network_address(),
            p2p_address: sui_config::utils::new_udp_network_address(),
            narwhal_primary_address: sui_config::utils::new_udp_network_address(),
            narwhal_worker_address: sui_config::utils::new_udp_network_address(),
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
    let (aggregator, authorities) = init_local_authorities_with_genesis(&genesis, key_pairs).await;
    (aggregator, authorities, genesis, framework)
}

pub async fn init_local_authorities_with_genesis(
    genesis: &Genesis,
    key_pairs: Vec<(AuthorityPublicKeyBytes, AuthorityKeyPair)>,
) -> (
    AuthorityAggregator<LocalAuthorityClient>,
    Vec<Arc<AuthorityState>>,
) {
    telemetry_subscribers::init_for_testing();
    let committee = genesis.committee().unwrap();

    let mut clients = BTreeMap::new();
    let mut states = Vec::new();
    for (authority_name, secret) in key_pairs {
        let client = LocalAuthorityClient::new(committee.clone(), secret, genesis).await;
        states.push(client.state.clone());
        clients.insert(authority_name, client);
    }
    let timeouts = TimeoutConfig {
        authority_request_timeout: Duration::from_secs(5),
        pre_quorum_timeout: Duration::from_secs(5),
        post_quorum_timeout: Duration::from_secs(5),
        serial_authority_request_timeout: Duration::from_secs(1),
        serial_authority_request_interval: Duration::from_secs(1),
    };
    let committee_store = Arc::new(CommitteeStore::new_for_testing(&committee));
    (
        AuthorityAggregator::new_with_timeouts(
            committee,
            committee_store,
            clients,
            &Registry::new(),
            timeouts,
        ),
        states,
    )
}

pub fn make_transfer_sui_transaction(
    gas_object: ObjectRef,
    recipient: SuiAddress,
    amount: Option<u64>,
    sender: SuiAddress,
    keypair: &AccountKeyPair,
    gas_price: u64,
) -> VerifiedTransaction {
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
) -> VerifiedTransaction {
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
) -> VerifiedTransaction {
    let data = TransactionData::new_transfer(
        recipient,
        object_ref,
        sender,
        gas_object,
        gas_price * TEST_ONLY_GAS_UNIT_FOR_TRANSFER,
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
    gas_price: u64,
) -> VerifiedTransaction {
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
            TEST_ONLY_GAS_UNIT_FOR_OBJECT_BASICS * gas_price,
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
) -> VerifiedTransaction {
    Transaction::from_data_and_signer(
        TransactionData::new_transfer(
            receiver,
            random_object_ref(),
            sender,
            random_object_ref(),
            TEST_ONLY_GAS_UNIT_FOR_TRANSFER * 10,
            10,
        ),
        Intent::sui_transaction(),
        vec![sender_sec],
    )
    .verify()
    .unwrap()
}

/// Make a cert using an arbitrarily large committee.
pub fn make_cert_with_large_committee(
    committee: &Committee,
    key_pairs: &[AuthorityKeyPair],
    transaction: &VerifiedTransaction,
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
                transaction.clone().into_message(),
                key_pair,
                AuthorityPublicKeyBytes::from(key_pair.public()),
            )
            .auth_sig()
            .clone()
        })
        .collect();

    let cert =
        CertifiedTransaction::new(transaction.clone().into_message(), sigs, committee).unwrap();
    cert.verify_signature(committee).unwrap();
    cert
}
