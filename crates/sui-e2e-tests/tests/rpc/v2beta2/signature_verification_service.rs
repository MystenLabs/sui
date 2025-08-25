// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

use shared_crypto::intent::{Intent, IntentMessage};
use sui_macros::sim_test;
use sui_rpc::proto::sui::rpc::v2beta2::signature_verification_service_client::SignatureVerificationServiceClient;
use sui_rpc::proto::sui::rpc::v2beta2::Bcs;
use sui_rpc::proto::sui::rpc::v2beta2::UserSignature;
use sui_rpc::proto::sui::rpc::v2beta2::VerifySignatureRequest;
use sui_test_transaction_builder::TestTransactionBuilder;
use sui_types::base_types::SuiAddress;
use sui_types::crypto::Signature;
use sui_types::signature::GenericSignature;
use sui_types::zk_login_authenticator::ZkLoginAuthenticator;
use test_cluster::TestClusterBuilder;

#[sim_test]
async fn test_verify_signature_zklogin() -> Result<(), anyhow::Error> {
    if sui_simulator::has_mainnet_protocol_config_override() {
        return Ok(());
    }

    let test_cluster = TestClusterBuilder::new()
        .with_epoch_duration_ms(10000)
        .with_default_jwks()
        .build()
        .await;
    test_cluster.wait_for_epoch(Some(1)).await;
    test_cluster.wait_for_authenticator_state_update().await;

    let mut client = SignatureVerificationServiceClient::connect(test_cluster.rpc_url().to_owned())
        .await
        .unwrap();

    // Construct a valid zkLogin transaction data, signature.
    let (kp, pk_zklogin, inputs) = &sui_types::utils::load_test_vectors(
        "../sui-types/src/unit_tests/zklogin_test_vectors.json",
    )[1];

    let zklogin_addr = (pk_zklogin).into();
    let rgp = test_cluster.get_reference_gas_price().await;
    let gas = test_cluster
        .fund_address_and_return_gas(rgp, Some(20000000000), zklogin_addr)
        .await;

    let tx_data = TestTransactionBuilder::new(zklogin_addr, gas, rgp)
        .transfer_sui(None, SuiAddress::ZERO)
        .build();
    let msg = IntentMessage::new(Intent::sui_transaction(), tx_data.clone());
    let eph_sig = Signature::new_secure(&msg, kp);
    let generic_sig = GenericSignature::ZkLoginAuthenticator(ZkLoginAuthenticator::new(
        inputs.clone(),
        2,
        eph_sig.clone(),
    ));

    // construct all parameters for the query
    let message = Bcs::from(bcs::to_bytes(&tx_data).unwrap());
    let signature = Bcs::from(generic_sig.as_ref().to_owned());

    let response = client
        .verify_signature(VerifySignatureRequest {
            message: Some(message.clone()),
            signature: Some(UserSignature {
                bcs: Some(signature.clone()),
                ..Default::default()
            }),
            ..Default::default()
        })
        .await
        .unwrap()
        .into_inner();
    assert_eq!(response.is_valid, Some(true));
    assert_eq!(response.reason, None);

    // address checks pass
    let response = client
        .verify_signature(VerifySignatureRequest {
            message: Some(message.clone()),
            signature: Some(UserSignature {
                bcs: Some(signature.clone()),
                ..Default::default()
            }),
            address: Some(zklogin_addr.to_string()),
            ..Default::default()
        })
        .await
        .unwrap()
        .into_inner();
    assert_eq!(response.is_valid, Some(true));
    assert_eq!(response.reason, None);

    // address checks fail
    let wrong_address = SuiAddress::random_for_testing_only();
    let response = client
        .verify_signature(VerifySignatureRequest {
            message: Some(message.clone()),
            signature: Some(UserSignature {
                bcs: Some(signature.clone()),
                ..Default::default()
            }),
            address: Some(wrong_address.to_string()),
            ..Default::default()
        })
        .await
        .unwrap()
        .into_inner();
    assert_eq!(response.is_valid, Some(false));
    assert_eq!(
        response.reason,
        Some(format!(
            "provided address `{}` does not match derived address `{}`",
            wrong_address, zklogin_addr
        ))
    );

    // Use the same signature but a different message so force verification failure
    let response = client
        .verify_signature(VerifySignatureRequest {
            message: Some(Bcs::from(bcs::to_bytes("some personal message").unwrap())),
            signature: Some(UserSignature {
                bcs: Some(signature),
                ..Default::default()
            }),
            ..Default::default()
        })
        .await
        .unwrap()
        .into_inner();
    assert_eq!(response.is_valid, Some(false));
    assert!(response.reason.is_some());

    Ok(())
}
