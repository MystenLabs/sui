// Copyright (c) 2022, Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

use super::*;
use crate::authority::authority_tests::get_genesis_package_by_module;
use crate::authority::authority_tests::{init_state, init_state_with_objects};
use move_core_types::account_address::AccountAddress;
use move_core_types::ident_str;
use std::time::Duration;
use sui_adapter::genesis;
use sui_network::transport;
use sui_types::base_types::{ObjectID, SuiAddress, TransactionDigest};
use sui_types::crypto::{KeyPair, Signature};
use sui_types::gas_coin::GasCoin;
use sui_types::messages::{
    CertifiedTransaction, SignatureAggregator, Transaction, TransactionData,
};
use sui_types::object::{MoveObject, Object, Owner};
use sui_types::serialize::serialize_cert;
use test_utils::{sequencer::Sequencer, test_keys};

/// Default network buffer size.
const NETWORK_BUFFER_SIZE: usize = 65_000;

/// Fixture: a test keypair.
pub fn test_keypair() -> (SuiAddress, KeyPair) {
    test_keys().pop().unwrap()
}

/// Fixture: a few test gas objects.
pub fn test_gas_objects() -> Vec<Object> {
    (0..4)
        .map(|i| {
            let seed = format!("0x555555555555555{i}");
            let gas_object_id = ObjectID::from_hex_literal(&seed).unwrap();
            let (sender, _) = test_keypair();
            Object::with_id_owner_for_testing(gas_object_id, sender)
        })
        .collect()
}

/// Fixture: a a test shared object.
pub fn test_shared_object() -> Object {
    let seed = "0x6666666666666660";
    let shared_object_id = ObjectID::from_hex_literal(seed).unwrap();
    let content = GasCoin::new(shared_object_id, SequenceNumber::new(), 10);
    let obj = MoveObject::new(/* type */ GasCoin::type_(), content.to_bcs_bytes());
    Object::new_move(obj, Owner::SharedMutable, TransactionDigest::genesis())
}

/// Fixture: a few test certificates containing a shared object.
pub async fn test_certificates(authority: &AuthorityState) -> Vec<CertifiedTransaction> {
    let (sender, keypair) = test_keypair();

    let mut certificates = Vec::new();
    let shared_object_id = test_shared_object().id();
    for gas_object in test_gas_objects() {
        // Make a sample transaction.
        let module = "ObjectBasics";
        let function = "create";
        let genesis_package_objects = genesis::clone_genesis_packages();
        let package_object_ref = get_genesis_package_by_module(&genesis_package_objects, module);

        let data = TransactionData::new_move_call(
            sender,
            package_object_ref,
            ident_str!(module).to_owned(),
            ident_str!(function).to_owned(),
            /* type_args */ vec![],
            gas_object.compute_object_reference(),
            /* object_args */ vec![],
            vec![shared_object_id],
            /* pure_args */
            vec![
                16u64.to_le_bytes().to_vec(),
                bcs::to_bytes(&AccountAddress::from(sender)).unwrap(),
            ],
            /* max_gas */ 10_000,
        );
        let signature = Signature::new(&data, &keypair);
        let transaction = Transaction::new(data, signature);

        // Submit the transaction and assemble a certificate.
        let response = authority
            .handle_transaction(transaction.clone())
            .await
            .unwrap();
        let vote = response.signed_transaction.unwrap();
        let certificate = SignatureAggregator::try_new(transaction, &authority.committee)
            .unwrap()
            .append(vote.auth_signature.authority, vote.auth_signature.signature)
            .unwrap()
            .unwrap();
        certificates.push(certificate);
    }
    certificates
}

#[tokio::test]
async fn handle_consensus_output() {
    // Initialize an authority with a (owned) gas object and a shared object.
    let mut objects = test_gas_objects();
    objects.push(test_shared_object());
    let authority = init_state_with_objects(objects).await;

    // Make a sample certificate.
    let certificate = &test_certificates(&authority).await[0];
    let serialized_certificate = serialize_cert(certificate);

    // Spawn a sequencer.
    // TODO [issue #932]: Use a port allocator to avoid port conflicts.
    let consensus_input_address = "127.0.0.1:1309".parse().unwrap();
    let consensus_subscriber_address = "127.0.0.1:1310".parse().unwrap();
    let sequencer = Sequencer {
        input_address: consensus_input_address,
        subscriber_address: consensus_subscriber_address,
        buffer_size: NETWORK_BUFFER_SIZE,
        consensus_delay: Duration::from_millis(0),
    };
    let store_path = temp_testdir::TempDir::default();
    Sequencer::spawn(sequencer, store_path.as_ref())
        .await
        .unwrap();

    // Spawn a consensus client.
    let state = Arc::new(authority);
    let consensus_client = ConsensusClient::new(state.clone()).unwrap();
    ConsensusClient::spawn(
        consensus_client,
        consensus_subscriber_address,
        NETWORK_BUFFER_SIZE,
    );

    // Submit a certificate to the sequencer.
    tokio::task::yield_now().await;
    transport::connect(consensus_input_address.to_string(), NETWORK_BUFFER_SIZE)
        .await
        .unwrap()
        .write_data(&serialized_certificate)
        .await
        .unwrap();

    // Wait for the certificate to be processed and ensure the last consensus index is correctly updated.
    // (We need to wait on storage for that.)
    while state.db().last_consensus_index().unwrap() != SequenceNumber::from(1) {
        tokio::time::sleep(std::time::Duration::from_millis(10)).await;
    }

    // Cleanup the storage.
    let _ = std::fs::remove_dir_all(store_path);
}

#[tokio::test]
async fn test_guardrail() {
    let authority = init_state().await;
    let state = Arc::new(authority);

    // Create a first consensus client.
    let _consensus_client = ConsensusClient::new(state.clone()).unwrap();

    // Create a second consensus client from the same state.
    let result = ConsensusClient::new(state);
    assert!(result.is_err());
}

#[tokio::test]
async fn sync_with_consensus() {
    // Initialize an authority with a (owned) gas object and a shared object.
    let mut objects = test_gas_objects();
    objects.push(test_shared_object());
    let authority = init_state_with_objects(objects).await;

    // Make two certificates.
    let certificate_0 = &test_certificates(&authority).await[0];
    let serialized_certificate_0 = serialize_cert(certificate_0);
    let certificate_1 = &test_certificates(&authority).await[1];
    let serialized_certificate_1 = serialize_cert(certificate_1);

    // Spawn a sequencer.
    // TODO [issue #932]: Use a port allocator to avoid port conflicts.
    let consensus_input_address = "127.0.0.1:13011".parse().unwrap();
    let consensus_subscriber_address = "127.0.0.1:1312".parse().unwrap();
    let sequencer = Sequencer {
        input_address: consensus_input_address,
        subscriber_address: consensus_subscriber_address,
        buffer_size: NETWORK_BUFFER_SIZE,
        consensus_delay: Duration::from_millis(0),
    };
    let store_path = temp_testdir::TempDir::default();
    Sequencer::spawn(sequencer, store_path.as_ref())
        .await
        .unwrap();

    // Submit a certificate to the sequencer.
    tokio::task::yield_now().await;
    transport::connect(consensus_input_address.to_string(), NETWORK_BUFFER_SIZE)
        .await
        .unwrap()
        .write_data(&serialized_certificate_0)
        .await
        .unwrap();

    // Spawn a consensus client.
    let state = Arc::new(authority);
    let consensus_client = ConsensusClient::new(state.clone()).unwrap();
    ConsensusClient::spawn(
        consensus_client,
        consensus_subscriber_address,
        NETWORK_BUFFER_SIZE,
    );

    // Submit a second certificate to the sequencer. This will force the consensus client to sync.
    transport::connect(consensus_input_address.to_string(), NETWORK_BUFFER_SIZE)
        .await
        .unwrap()
        .write_data(&serialized_certificate_1)
        .await
        .unwrap();

    // Wait for the certificate to be processed and ensure the last consensus index is correctly updated.
    // (We need to wait on storage for that.)
    while state.db().last_consensus_index().unwrap() != SequenceNumber::from(2) {
        tokio::time::sleep(std::time::Duration::from_millis(10)).await;
    }

    // Ensure the version number of the shared object is correctly updated.
    let shared_object_id = test_shared_object().id();
    let version = state.db().get_schedule(&shared_object_id).unwrap().unwrap();
    assert_eq!(version, SequenceNumber::from(2));

    // Ensure the certificates are locked in the right order.
    let certificate_0_sequence = state
        .db()
        .sequenced(certificate_0.digest(), vec![shared_object_id].iter())
        .unwrap()
        .pop()
        .unwrap()
        .unwrap();
    assert_eq!(certificate_0_sequence, SequenceNumber::default());

    let certificate_1_sequence = state
        .db()
        .sequenced(certificate_1.digest(), vec![shared_object_id].iter())
        .unwrap()
        .pop()
        .unwrap()
        .unwrap();
    assert_eq!(certificate_1_sequence, SequenceNumber::from(1));

    // Cleanup the storage.
    let _ = std::fs::remove_dir_all(store_path);
}
