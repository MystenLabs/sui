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
    Bcs, ExecuteTransactionRequest, ExecuteTransactionResponse, SignatureScheme, Transaction,
    UserSignature,
};
use sui_test_transaction_builder::TestTransactionBuilder;
use sui_types::base_types::SuiAddress;
use sui_types::crypto::{Signature, SuiKeyPair};
use sui_types::signature::GenericSignature;
use test_cluster::TestClusterBuilder;

/// Devnet submits through gRPC, so the ML-DSA-65 scheme byte has to survive
/// the proto round trip, not only the validator API covered by `mldsa_tests`.
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
    let sender = SuiAddress::from(&kp.public());
    let rgp = test_cluster.get_reference_gas_price().await;
    let gas = test_cluster
        .fund_address_and_return_gas(rgp, Some(20000000000), sender)
        .await;
    let tx_data = TestTransactionBuilder::new(sender, gas, rgp)
        .transfer_sui(Some(9), SuiAddress::ZERO)
        .build();
    let intent_msg = IntentMessage::new(Intent::sui_transaction(), tx_data);
    let sig = GenericSignature::Signature(Signature::new_secure(&intent_msg, &kp));

    let mut client = TransactionExecutionServiceClient::connect(test_cluster.rpc_url().to_owned())
        .await
        .unwrap();
    let ExecuteTransactionResponse { transaction, .. } = client
        .execute_transaction({
            let mut message = ExecuteTransactionRequest::default();
            message.transaction = Some({
                let mut message = Transaction::default();
                message.bcs = Some(Bcs::serialize(&intent_msg.value).unwrap());
                message
            });
            message.signatures = vec![{
                let mut message = UserSignature::default();
                message.bcs = Some(Bcs::from(sig.as_ref().to_owned()));
                message
            }];
            message.read_mask = Some(FieldMask::from_paths(["*"]));
            message
        })
        .await
        .unwrap()
        .into_inner();

    let transaction = transaction.unwrap();
    let effects = transaction.effects.unwrap();
    assert!(effects.status().success(), "{:?}", effects.status());

    // The response renders the signature with its own scheme and raw parts.
    let signature = &transaction.signatures[0];
    assert_eq!(signature.scheme(), SignatureScheme::Mldsa65);
    let simple = signature.simple().clone();
    assert_eq!(simple.scheme(), SignatureScheme::Mldsa65);
    assert_eq!(simple.signature().len(), 3309);
    assert_eq!(simple.public_key().len(), 1952);
}
