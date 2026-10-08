// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

use fastcrypto::traits::KeyPair as _;
use fastcrypto_pq::mldsa65::MLDSA65KeyPair;
use prost_types::FieldMask;
use shared_crypto::intent::{Intent, IntentMessage};
use sui_macros::sim_test;
use sui_protocol_config::ProtocolConfig;
use sui_rpc::field::FieldMaskUtil;
use sui_rpc::proto::sui::rpc::v2::transaction_execution_service_client::TransactionExecutionServiceClient;
use sui_rpc::proto::sui::rpc::v2::{
    Bcs, ExecuteTransactionRequest, ExecuteTransactionResponse, ExecutedTransaction,
    SignatureScheme, SimpleSignature, Transaction, UserSignature, user_signature,
};
use sui_test_transaction_builder::TestTransactionBuilder;
use sui_types::base_types::SuiAddress;
use sui_types::crypto::{Signature, SuiKeyPair};
use sui_types::transaction::TransactionData;
use test_cluster::{TestCluster, TestClusterBuilder};

/// Devnet submits through gRPC, so the ML-DSA-65 scheme byte has to survive
/// the proto round trip, not only the validator API covered by `mldsa_tests`.
/// The signature is submitted both as BCS and as structured proto fields, so
/// the server-side reconstruction from fields is exercised too.
#[sim_test]
async fn execute_transaction_mldsa65() {
    let _guard = ProtocolConfig::apply_overrides_for_testing(|_, mut config| {
        config.set_mldsa65_auth_for_testing(true);
        config
    });
    let test_cluster = TestClusterBuilder::new()
        .with_num_validators(1)
        .build()
        .await;
    let kp = SuiKeyPair::MLDSA65(MLDSA65KeyPair::generate(&mut rand::thread_rng()));

    // BCS envelope, the path wallets use today.
    let (tx_data, sig) = signed_transfer(&test_cluster, &kp).await;
    let executed = execute(&test_cluster, &tx_data, {
        let mut message = UserSignature::default();
        message.bcs = Some(Bcs::from(sig.as_ref().to_owned()));
        message
    })
    .await;
    assert_mldsa65_signature(&executed);

    // Structured fields only: the server rebuilds the signature from scheme,
    // signature bytes and public key, with no BCS to fall back on.
    let (tx_data, sig) = signed_transfer(&test_cluster, &kp).await;
    let executed = execute(&test_cluster, &tx_data, {
        let mut message = UserSignature::default();
        message.set_scheme(SignatureScheme::Mldsa65);
        message.signature = Some(user_signature::Signature::Simple(SimpleSignature::from(
            &sig,
        )));
        message
    })
    .await;
    assert_mldsa65_signature(&executed);
}

/// Fund `kp`'s address and sign a small transfer from it.
async fn signed_transfer(
    test_cluster: &TestCluster,
    kp: &SuiKeyPair,
) -> (TransactionData, Signature) {
    let sender = SuiAddress::from(&kp.public());
    let rgp = test_cluster.get_reference_gas_price().await;
    let gas = test_cluster
        .fund_address_and_return_gas(rgp, Some(20000000000), sender)
        .await;
    let tx_data = TestTransactionBuilder::new(sender, gas, rgp)
        .transfer_sui(Some(9), SuiAddress::ZERO)
        .build();
    let intent_msg = IntentMessage::new(Intent::sui_transaction(), tx_data);
    let sig = Signature::new_secure(&intent_msg, kp);
    (intent_msg.value, sig)
}

async fn execute(
    test_cluster: &TestCluster,
    tx_data: &TransactionData,
    signature: UserSignature,
) -> ExecutedTransaction {
    let mut client = TransactionExecutionServiceClient::connect(test_cluster.rpc_url().to_owned())
        .await
        .unwrap();
    let ExecuteTransactionResponse { transaction, .. } = client
        .execute_transaction({
            let mut message = ExecuteTransactionRequest::default();
            message.transaction = Some({
                let mut message = Transaction::default();
                message.bcs = Some(Bcs::serialize(tx_data).unwrap());
                message
            });
            message.signatures = vec![signature];
            message.read_mask = Some(FieldMask::from_paths(["*"]));
            message
        })
        .await
        .unwrap()
        .into_inner();
    let transaction = transaction.unwrap();
    let effects = transaction.effects.as_ref().unwrap();
    assert!(effects.status().success(), "{:?}", effects.status());
    transaction
}

/// The response renders the signature with its own scheme and raw parts.
fn assert_mldsa65_signature(transaction: &ExecutedTransaction) {
    let signature = &transaction.signatures[0];
    assert_eq!(signature.scheme(), SignatureScheme::Mldsa65);
    let simple = signature.simple();
    assert_eq!(simple.scheme(), SignatureScheme::Mldsa65);
    assert_eq!(simple.signature().len(), 3309);
    assert_eq!(simple.public_key().len(), 1952);
}
