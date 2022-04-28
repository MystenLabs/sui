// Copyright (c) 2022, Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0
use super::*;
use crate::authority::authority_tests::get_genesis_package_by_module;
use crate::authority::authority_tests::init_state_with_objects;
use crate::authority::AuthorityState;
use move_core_types::account_address::AccountAddress;
use move_core_types::ident_str;
use sui_adapter::genesis;
use sui_types::base_types::SequenceNumber;
use sui_types::base_types::{ObjectID, TransactionDigest};
use sui_types::crypto::Signature;
use sui_types::gas_coin::GasCoin;
use sui_types::messages::{
    CallArg, CertifiedTransaction, SignatureAggregator, Transaction, TransactionData,
};
use sui_types::object::{MoveObject, Object, Owner};
use test_utils::network::test_listener;
use test_utils::test_keys;
use tokio::sync::mpsc::channel;

/// Default network buffer size.
const NETWORK_BUFFER_SIZE: usize = 65_000;

/// Fixture: a few test gas objects.
pub fn test_gas_objects() -> Vec<Object> {
    (0..4)
        .map(|i| {
            let seed = format!("0x555555555555555{i}");
            let gas_object_id = ObjectID::from_hex_literal(&seed).unwrap();
            let (sender, _) = test_keys().pop().unwrap();
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
    Object::new_move(obj, Owner::Shared, TransactionDigest::genesis())
}

/// Fixture: a few test certificates containing a shared object.
pub async fn test_certificates(authority: &AuthorityState) -> Vec<CertifiedTransaction> {
    let (sender, keypair) = test_keys().pop().unwrap();

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
            /* args */
            vec![
                CallArg::SharedObject(shared_object_id),
                CallArg::Pure(16u64.to_le_bytes().to_vec()),
                CallArg::Pure(bcs::to_bytes(&AccountAddress::from(sender)).unwrap()),
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
            .append(vote.auth_sign_info.authority, vote.auth_sign_info.signature)
            .unwrap()
            .unwrap();
        certificates.push(certificate);
    }
    certificates
}

#[tokio::test]
async fn listen_to_sequenced_transaction() {
    let (tx_sui_to_consensus, rx_sui_to_consensus) = channel(1);
    let (tx_consensus_to_sui, rx_consensus_to_sui) = channel(1);

    // Make a sample (serialized) consensus transaction.
    let transaction = vec![10u8, 11u8];
    let transaction_digest = ConsensusListener::hash(&transaction);

    // Spawn a consensus listener.
    ConsensusListener::spawn(
        /* rx_consensus_input */ rx_sui_to_consensus,
        /* rx_consensus_output */ rx_consensus_to_sui,
    );

    // Submit a sample consensus transaction.
    let (sender, receiver) = oneshot::channel();
    let input = ConsensusInput {
        serialized: transaction.clone(),
        replier: sender,
    };
    tx_sui_to_consensus.send(input).await.unwrap();

    // Notify the consensus listener that the transaction has been sequenced.
    tokio::task::yield_now().await;
    let output = (Ok(()), transaction_digest);
    tx_consensus_to_sui.send(output).await.unwrap();

    // Ensure the caller get notified from the consensus listener.
    assert!(receiver.await.unwrap().is_ok());
}

#[tokio::test]
async fn submit_transaction_to_consensus() {
    // TODO [issue #932]: Use a port allocator to avoid port conflicts.
    let consensus_address = "127.0.0.1:12456".parse().unwrap();
    let (tx_consensus_listener, mut rx_consensus_listener) = channel(1);

    // Initialize an authority with a (owned) gas object and a shared object; then
    // make a test certificate.
    let mut objects = test_gas_objects();
    objects.push(test_shared_object());
    let authority = init_state_with_objects(objects).await;
    let certificate = test_certificates(&authority).await.pop().unwrap();
    let expected_transaction = certificate.transaction.clone();

    // Make a new consensus submitter instance.
    let submitter = ConsensusSubmitter::new(
        consensus_address,
        NETWORK_BUFFER_SIZE,
        authority.committee,
        tx_consensus_listener,
        /* max_delay */ Duration::from_millis(1_000),
    );

    // Spawn a network listener to receive the transaction (emulating the consensus node).
    let handle = test_listener(consensus_address);

    // Notify the submitter when a consensus transaction has been sequenced.
    tokio::spawn(async move {
        let ConsensusInput { replier, .. } = rx_consensus_listener.recv().await.unwrap();
        replier.send(Ok(())).unwrap();
    });

    // Submit the transaction and ensure the submitter reports success to the caller.
    tokio::task::yield_now().await;
    let consensus_transaction = ConsensusTransaction::UserTransaction(certificate);
    let result = submitter.submit(&consensus_transaction).await;
    assert!(result.is_ok());

    // Ensure the consensus node got the transaction.
    let bytes = handle.await.unwrap();
    match bincode::deserialize(&bytes).unwrap() {
        ConsensusTransaction::UserTransaction(x) => {
            assert_eq!(x.transaction, expected_transaction)
        }
    }
}
