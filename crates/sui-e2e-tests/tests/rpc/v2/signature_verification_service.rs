// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

use fastcrypto::ed25519::Ed25519KeyPair;
use fastcrypto::traits::KeyPair;
use fastcrypto_zkp::bn254::zk_login::ZkLoginInputs;
use rand::SeedableRng;
use rand::rngs::StdRng;
use shared_crypto::intent::{Intent, IntentMessage};
use sui_macros::sim_test;
use sui_rpc::proto::sui::rpc::v2::Bcs;
use sui_rpc::proto::sui::rpc::v2::UserSignature;
use sui_rpc::proto::sui::rpc::v2::VerifySignatureRequest;
use sui_rpc::proto::sui::rpc::v2::signature_verification_service_client::SignatureVerificationServiceClient;
use sui_rpc::proto::sui::rpc::v2::{ActiveJwk, Jwk, JwkId};
use sui_test_transaction_builder::TestTransactionBuilder;
use sui_types::base_types::{ObjectDigest, ObjectID, SuiAddress};
use sui_types::crypto::{PublicKey, Signature, SuiKeyPair};
use sui_types::signature::GenericSignature;
use sui_types::transaction::TransactionData;
use sui_types::utils::{
    PINNED_PROOF_ADDRESS_SEED, PINNED_V1_PROOF_JSON, PINNED_V2_PROOF_JSON, pinned_jwks,
};
use sui_types::zk_login_authenticator::ZkLoginAuthenticator;
use test_cluster::TestClusterBuilder;

#[sim_test]
async fn test_verify_signature_zklogin() -> Result<(), anyhow::Error> {
    if sui_simulator::has_mainnet_protocol_config_override() {
        return Ok(());
    }

    let test_cluster = TestClusterBuilder::new()
        .with_num_validators(1)
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
        .verify_signature({
            let mut request = VerifySignatureRequest::default();
            request.message = Some(message.clone());
            request.signature = Some({
                let mut message = UserSignature::default();
                message.bcs = Some(signature.clone());
                message
            });
            request
        })
        .await
        .unwrap()
        .into_inner();
    assert_eq!(response.is_valid, Some(true));
    assert_eq!(response.reason, None);

    // address checks pass
    let response = client
        .verify_signature({
            let mut request = VerifySignatureRequest::default();
            request.message = Some(message.clone());
            request.signature = Some({
                let mut message = UserSignature::default();
                message.bcs = Some(signature.clone());
                message
            });
            request.address = Some(zklogin_addr.to_string());
            request
        })
        .await
        .unwrap()
        .into_inner();
    assert_eq!(response.is_valid, Some(true));
    assert_eq!(response.reason, None);

    // address checks fail
    let wrong_address = SuiAddress::random_for_testing_only();
    let response = client
        .verify_signature({
            let mut request = VerifySignatureRequest::default();
            request.message = Some(message.clone());
            request.signature = Some({
                let mut message = UserSignature::default();
                message.bcs = Some(signature.clone());
                message
            });
            request.address = Some(wrong_address.to_string());
            request
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
        .verify_signature({
            let mut message = VerifySignatureRequest::default();
            message.message = Some(Bcs::from(bcs::to_bytes("some personal message").unwrap()));
            message.signature = Some({
                let mut message = UserSignature::default();
                message.bcs = Some(signature);
                message
            });
            message
        })
        .await
        .unwrap()
        .into_inner();
    assert_eq!(response.is_valid, Some(false));
    assert!(response.reason.is_some());

    // Verify zklogin signatures carrying the pinned v1 and v2 circuit proofs, with the
    // matching test-issuer JWKs supplied in the request. In circuit mode 1 both proofs verify.
    let jwks: Vec<ActiveJwk> = pinned_jwks()
        .into_iter()
        .map(|(id, jwk)| {
            let mut active = ActiveJwk::default();
            active.id = Some({
                let mut proto = JwkId::default();
                proto.iss = Some(id.iss);
                proto.kid = Some(id.kid);
                proto
            });
            active.jwk = Some({
                let mut proto = Jwk::default();
                proto.kty = Some(jwk.kty);
                proto.e = Some(jwk.e);
                proto.n = Some(jwk.n);
                proto.alg = Some(jwk.alg);
                proto
            });
            active.epoch = Some(0);
            active
        })
        .collect();

    for proof_json in [PINNED_V1_PROOF_JSON, PINNED_V2_PROOF_JSON] {
        // Both pinned proofs use ephemeral key seed [0; 32] and max_epoch 10.
        let inputs = ZkLoginInputs::from_json(proof_json, PINNED_PROOF_ADDRESS_SEED)?;
        let eph_kp = SuiKeyPair::Ed25519(Ed25519KeyPair::generate(&mut StdRng::from_seed([0; 32])));
        let zklogin_addr = SuiAddress::from(&PublicKey::from_zklogin_inputs(&inputs)?);

        let gas_ref = (ObjectID::random(), 1.into(), ObjectDigest::random());
        let tx_data = TransactionData::new_transfer_sui(
            SuiAddress::ZERO,
            zklogin_addr,
            Some(1),
            gas_ref,
            5_000_000,
            1000,
        );
        let msg = IntentMessage::new(Intent::sui_transaction(), tx_data.clone());
        let eph_sig = Signature::new_secure(&msg, &eph_kp);
        let generic_sig =
            GenericSignature::ZkLoginAuthenticator(ZkLoginAuthenticator::new(inputs, 10, eph_sig));

        let message = {
            let mut bcs = Bcs::from(bcs::to_bytes(&tx_data)?);
            bcs.name = Some("TransactionData".to_string());
            bcs
        };
        let signature = {
            let mut signature = UserSignature::default();
            signature.bcs = Some(Bcs::from(generic_sig.as_ref().to_owned()));
            signature
        };

        let response = client
            .verify_signature({
                let mut request = VerifySignatureRequest::default();
                request.message = Some(message);
                request.signature = Some(signature.clone());
                request.address = Some(zklogin_addr.to_string());
                request.jwks = jwks.clone();
                request
            })
            .await
            .unwrap()
            .into_inner();
        assert_eq!(response.reason, None);
        assert_eq!(response.is_valid, Some(true));

        // The same signature over a different message must not verify.
        let response = client
            .verify_signature({
                let mut request = VerifySignatureRequest::default();
                request.message = Some(Bcs::from(bcs::to_bytes("some personal message")?));
                request.signature = Some(signature);
                request.jwks = jwks.clone();
                request
            })
            .await
            .unwrap()
            .into_inner();
        assert_eq!(response.is_valid, Some(false));
        assert!(response.reason.is_some());
    }

    Ok(())
}
