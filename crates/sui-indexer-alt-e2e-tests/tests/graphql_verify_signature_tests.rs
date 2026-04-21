// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

use std::time::Duration;

use anyhow::Context as _;
use anyhow::bail;
use fastcrypto::encoding::Base64;
use fastcrypto::encoding::Encoding;
use fastcrypto::traits::KeyPair;
use fastcrypto::traits::ToFromBytes;
use insta::assert_debug_snapshot;
use prometheus::Registry;
use rand::SeedableRng;
use serde_json::json;
use shared_crypto::intent::Intent;
use shared_crypto::intent::IntentMessage;
use shared_crypto::intent::PersonalMessage;
use sui_indexer_alt_e2e_tests::OffchainCluster;
use sui_indexer_alt_e2e_tests::OffchainClusterConfig;
use sui_indexer_alt_framework::ingestion::ClientArgs;
use sui_indexer_alt_framework::ingestion::ingestion_client::IngestionClientArgs;
use sui_indexer_alt_graphql::config::RpcConfig as GraphQlConfig;
use sui_indexer_alt_graphql::config::ZkLoginConfig;
use sui_indexer_alt_graphql::config::ZkLoginEnv;
use sui_swarm_config::genesis_config::AccountConfig;
use sui_types::base_types::SuiAddress;
use sui_types::crypto::Signature;
use sui_types::crypto::SuiKeyPair;
use sui_types::multisig::MultiSig;
use sui_types::multisig::MultiSigPublicKey;
use sui_types::signature::GenericSignature;
use sui_types::utils::load_test_vectors;
use sui_types::zk_login_authenticator::ZkLoginAuthenticator;
use tempfile::TempDir;
use test_cluster::TestCluster;
use test_cluster::TestClusterBuilder;
use tokio::time::interval;

const SCOPE_PERSONAL_MESSAGE: &str = "PERSONAL_MESSAGE";
const SCOPE_TRANSACTION_DATA: &str = "TRANSACTION_DATA";

const QUERY: &str = r#"
query ($message: Base64!, $signature: Base64!, $scope: IntentScope!, $author: SuiAddress!) {
    verifySignature(message: $message, signature: $signature, intentScope: $scope, author: $author) {
        success
    }
}
"#;

struct FullCluster {
    #[allow(unused)]
    onchain: TestCluster,
    offchain: OffchainCluster,
    #[allow(unused)]
    temp_dir: TempDir,
}

impl FullCluster {
    async fn new() -> anyhow::Result<Self> {
        let temp_dir = TempDir::new()?;
        let ingestion_dir = temp_dir.path().to_path_buf();

        let onchain = TestClusterBuilder::new()
            .with_num_validators(1)
            .with_data_ingestion_dir(ingestion_dir.clone())
            .with_epoch_duration_ms(300_000)
            .with_accounts(vec![
                AccountConfig {
                    address: None,
                    gas_amounts: vec![1_000_000_000_000; 2],
                };
                4
            ])
            .build()
            .await;

        let offchain = OffchainCluster::new(
            ClientArgs {
                ingestion: IngestionClientArgs {
                    local_ingestion_path: Some(ingestion_dir),
                    ..Default::default()
                },
                ..Default::default()
            },
            OffchainClusterConfig {
                graphql_config: GraphQlConfig {
                    zklogin: ZkLoginConfig {
                        env: ZkLoginEnv::Test,
                        ..Default::default()
                    },
                    ..Default::default()
                },
                ..Default::default()
            },
            &Registry::new(),
        )
        .await?;

        // Trigger an epoch change and wait until GraphQL sees Epoch 1 (needed for JWK state).
        onchain.trigger_reconfiguration().await;
        onchain.wait_for_authenticator_state_update().await;
        tokio::time::timeout(Duration::from_secs(30), async {
            let mut interval = interval(Duration::from_millis(200));
            loop {
                interval.tick().await;
                if matches!(offchain.latest_graphql_epoch().await, Ok(1)) {
                    break;
                }
            }
        })
        .await
        .unwrap();

        Ok(Self {
            onchain,
            offchain,
            temp_dir,
        })
    }

    async fn verify(
        &self,
        message: Vec<u8>,
        signature: Vec<u8>,
        scope: &str,
        author: SuiAddress,
    ) -> anyhow::Result<serde_json::Value> {
        let client = reqwest::Client::new();
        let url = self.offchain.graphql_url();

        let variables = json!({
            "message": Base64::encode(message),
            "signature": Base64::encode(signature),
            "scope": scope,
            "author": author.to_string(),
        });

        let response: serde_json::Value = client
            .post(url.as_str())
            .json(&json!({
                "query": QUERY,
                "variables": variables,
            }))
            .send()
            .await?
            .json()
            .await?;

        if let Some(errors) = response.get("errors").and_then(|es| es.as_array()) {
            let errors: Vec<_> = errors
                .iter()
                .map(|e| e.get("message").unwrap().as_str().unwrap().to_owned())
                .collect();
            bail!(serde_json::to_string(&errors).unwrap());
        }

        response
            .pointer("/data/verifySignature")
            .cloned()
            .with_context(|| format!("missing data.verifySignature in {response:#?}"))
    }
}

/// Sign a personal message, returning (raw_message, signature_bytes, author_address).
fn sign_personal_message(keypair: &SuiKeyPair, msg: &[u8]) -> (Vec<u8>, Vec<u8>, SuiAddress) {
    let addr = SuiAddress::from(&keypair.public());
    let intent_msg = IntentMessage::new(
        Intent::personal_message(),
        PersonalMessage {
            message: msg.to_vec(),
        },
    );
    let sig = GenericSignature::Signature(Signature::new_secure(&intent_msg, keypair));
    (msg.to_vec(), sig.as_ref().to_owned(), addr)
}

fn ed25519_keypair(seed: u8) -> SuiKeyPair {
    SuiKeyPair::Ed25519(fastcrypto::ed25519::Ed25519KeyPair::generate(
        &mut rand::rngs::StdRng::from_seed([seed; 32]),
    ))
}

fn secp256k1_keypair(seed: u8) -> SuiKeyPair {
    SuiKeyPair::Secp256k1(fastcrypto::secp256k1::Secp256k1KeyPair::generate(
        &mut rand::rngs::StdRng::from_seed([seed; 32]),
    ))
}

fn secp256r1_keypair(seed: u8) -> SuiKeyPair {
    SuiKeyPair::Secp256r1(fastcrypto::secp256r1::Secp256r1KeyPair::generate(
        &mut rand::rngs::StdRng::from_seed([seed; 32]),
    ))
}

#[tokio::test]
async fn test_ed25519_personal_message() {
    let cluster = FullCluster::new().await.unwrap();
    let (message, signature, addr) =
        sign_personal_message(&ed25519_keypair(/* seed = */ 1), b"Hello, World!");

    let result = cluster
        .verify(message, signature, SCOPE_PERSONAL_MESSAGE, addr)
        .await
        .unwrap();

    insta::assert_json_snapshot!(result, @r###"
    {
      "success": true
    }
    "###);
}

#[tokio::test]
async fn test_secp256k1_personal_message() {
    let cluster = FullCluster::new().await.unwrap();
    let (message, signature, addr) =
        sign_personal_message(&secp256k1_keypair(/* seed = */ 2), b"Hello from Secp256k1!");

    let result = cluster
        .verify(message, signature, SCOPE_PERSONAL_MESSAGE, addr)
        .await
        .unwrap();

    insta::assert_json_snapshot!(result, @r###"
    {
      "success": true
    }
    "###);
}

#[tokio::test]
async fn test_secp256r1_personal_message() {
    let cluster = FullCluster::new().await.unwrap();
    let (message, signature, addr) =
        sign_personal_message(&secp256r1_keypair(/* seed = */ 3), b"Hello from Secp256r1!");

    let result = cluster
        .verify(message, signature, SCOPE_PERSONAL_MESSAGE, addr)
        .await
        .unwrap();

    insta::assert_json_snapshot!(result, @r###"
    {
      "success": true
    }
    "###);
}

#[tokio::test]
async fn test_multisig_personal_message() {
    let cluster = FullCluster::new().await.unwrap();

    // 2-of-3 multisig: Ed25519 + Secp256k1 + Secp256r1
    let kp1 = ed25519_keypair(/* seed = */ 10);
    let kp2 = secp256k1_keypair(/* seed = */ 11);
    let kp3 = secp256r1_keypair(/* seed = */ 12);
    let multisig_pk = MultiSigPublicKey::new(
        vec![kp1.public(), kp2.public(), kp3.public()],
        vec![1, 1, 1],
        /* threshold = */ 2,
    )
    .unwrap();
    let addr = SuiAddress::from(&multisig_pk);

    let personal = b"Hello from MultiSig!".to_vec();
    let intent_msg = IntentMessage::new(
        Intent::personal_message(),
        PersonalMessage {
            message: personal.clone(),
        },
    );
    let sig1 = GenericSignature::Signature(Signature::new_secure(&intent_msg, &kp1));
    let sig2 = GenericSignature::Signature(Signature::new_secure(&intent_msg, &kp2));
    let signature =
        GenericSignature::MultiSig(MultiSig::combine(vec![sig1, sig2], multisig_pk).unwrap());

    let result = cluster
        .verify(
            personal,
            signature.as_ref().to_owned(),
            SCOPE_PERSONAL_MESSAGE,
            addr,
        )
        .await
        .unwrap();

    insta::assert_json_snapshot!(result, @r###"
    {
      "success": true
    }
    "###);
}

#[tokio::test]
async fn test_zklogin_personal_message() {
    let cluster = FullCluster::new().await.unwrap();
    let (kp, pk, inputs) =
        &load_test_vectors("../sui-types/src/unit_tests/zklogin_test_vectors.json")[1];
    let addr: SuiAddress = pk.into();
    let personal = b"Hello from ZkLogin!".to_vec();

    let intent_msg = IntentMessage::new(
        Intent::personal_message(),
        PersonalMessage {
            message: personal.clone(),
        },
    );
    let signature = GenericSignature::ZkLoginAuthenticator(ZkLoginAuthenticator::new(
        inputs.clone(),
        2,
        Signature::new_secure(&intent_msg, kp),
    ));

    let result = cluster
        .verify(
            personal,
            signature.as_ref().to_owned(),
            SCOPE_PERSONAL_MESSAGE,
            addr,
        )
        .await
        .unwrap();

    insta::assert_json_snapshot!(result, @r###"
    {
      "success": true
    }
    "###);
}

// --- Passkey ---

/// Create a passkey credential and sign a personal message, returning (raw_message, signature_bytes, address).
async fn sign_passkey_personal_message(msg: &[u8]) -> (Vec<u8>, Vec<u8>, SuiAddress) {
    use passkey_authenticator::{Authenticator, UserCheck, UserValidationMethod};
    use passkey_client::Client;
    use passkey_types::{
        Bytes, Passkey,
        ctap2::{Aaguid, Ctap2Error},
        rand::random_vec,
        webauthn::{
            AttestationConveyancePreference, CredentialCreationOptions, CredentialRequestOptions,
            PublicKeyCredentialCreationOptions, PublicKeyCredentialParameters,
            PublicKeyCredentialRequestOptions, PublicKeyCredentialRpEntity,
            PublicKeyCredentialType, PublicKeyCredentialUserEntity, UserVerificationRequirement,
        },
    };
    use sui_types::crypto::{PublicKey, SignatureScheme};
    use sui_types::passkey_authenticator::PasskeyAuthenticator;

    struct TestUserValidation;
    #[async_trait::async_trait]
    impl UserValidationMethod for TestUserValidation {
        type PasskeyItem = Passkey;
        async fn check_user<'a>(
            &self,
            _credential: Option<&'a Passkey>,
            presence: bool,
            verification: bool,
        ) -> Result<UserCheck, Ctap2Error> {
            Ok(UserCheck {
                presence,
                verification,
            })
        }
        fn is_verification_enabled(&self) -> Option<bool> {
            Some(true)
        }
        fn is_presence_enabled(&self) -> bool {
            true
        }
    }

    let origin = url::Url::parse("https://www.sui.io").unwrap();

    // Create credential.
    let my_authenticator =
        Authenticator::new(Aaguid::new_empty(), None::<Passkey>, TestUserValidation);
    let mut client = Client::new(my_authenticator);
    let credential = client
        .register(
            &origin,
            CredentialCreationOptions {
                public_key: PublicKeyCredentialCreationOptions {
                    rp: PublicKeyCredentialRpEntity {
                        id: None,
                        name: origin.domain().unwrap().into(),
                    },
                    user: PublicKeyCredentialUserEntity {
                        id: random_vec(32).into(),
                        display_name: "Test".into(),
                        name: "test@example.org".into(),
                    },
                    challenge: random_vec(32).into(),
                    pub_key_cred_params: vec![PublicKeyCredentialParameters {
                        ty: PublicKeyCredentialType::PublicKey,
                        alg: coset::iana::Algorithm::ES256,
                    }],
                    timeout: None,
                    exclude_credentials: None,
                    authenticator_selection: None,
                    hints: None,
                    attestation: AttestationConveyancePreference::None,
                    attestation_formats: None,
                    extensions: None,
                },
            },
            None,
        )
        .await
        .unwrap();

    // Derive compact public key and address.
    use p256::pkcs8::DecodePublicKey;
    let verifying_key = p256::ecdsa::VerifyingKey::from_public_key_der(
        credential.response.public_key.unwrap().as_slice(),
    )
    .unwrap();
    let encoded_point = verifying_key.to_encoded_point(false);
    let x = encoded_point.x().unwrap();
    let y = encoded_point.y().unwrap();
    let prefix = if y[31] % 2 == 0 { 0x02 } else { 0x03 };
    let mut pk_bytes = vec![prefix];
    pk_bytes.extend_from_slice(x);
    let pk = PublicKey::try_from_bytes(SignatureScheme::PasskeyAuthenticator, &pk_bytes).unwrap();
    let sender = SuiAddress::from(&pk);

    // Sign the personal message with passkey.
    let intent_msg = IntentMessage::new(
        Intent::personal_message(),
        PersonalMessage {
            message: msg.to_vec(),
        },
    );
    let passkey_digest = sui_types::passkey_authenticator::to_signing_message(&intent_msg);

    let authenticated = client
        .authenticate(
            &origin,
            CredentialRequestOptions {
                public_key: PublicKeyCredentialRequestOptions {
                    challenge: Bytes::from(passkey_digest.to_vec()),
                    timeout: None,
                    rp_id: Some(String::from(origin.domain().unwrap())),
                    allow_credentials: None,
                    user_verification: UserVerificationRequirement::default(),
                    attestation: Default::default(),
                    attestation_formats: None,
                    extensions: None,
                    hints: None,
                },
            },
            None,
        )
        .await
        .unwrap();

    // Build the GenericSignature.
    let sig =
        p256::ecdsa::Signature::from_der(authenticated.response.signature.as_slice()).unwrap();
    let sig_bytes = sig.normalize_s().unwrap_or(sig).to_bytes();
    let mut user_sig_bytes = vec![SignatureScheme::Secp256r1.flag()];
    user_sig_bytes.extend_from_slice(&sig_bytes);
    user_sig_bytes.extend_from_slice(&pk_bytes);

    let passkey = PasskeyAuthenticator::new_for_testing(
        authenticated.response.authenticator_data.to_vec(),
        String::from_utf8_lossy(authenticated.response.client_data_json.as_slice()).to_string(),
        Signature::from_bytes(&user_sig_bytes).unwrap(),
    )
    .unwrap();
    let signature = GenericSignature::PasskeyAuthenticator(passkey);

    (msg.to_vec(), signature.as_ref().to_owned(), sender)
}

#[tokio::test]
async fn test_passkey_personal_message() {
    let cluster = FullCluster::new().await.unwrap();
    let (message, signature, addr) = sign_passkey_personal_message(b"Hello from Passkey!").await;

    let result = cluster
        .verify(message, signature, SCOPE_PERSONAL_MESSAGE, addr)
        .await
        .unwrap();

    insta::assert_json_snapshot!(result, @r###"
    {
      "success": true
    }
    "###);
}

// --- Error cases ---

#[tokio::test]
async fn test_invalid_signature_bytes() {
    let cluster = FullCluster::new().await.unwrap();

    let result = cluster
        .verify(
            b"Hello".to_vec(),
            vec![0xFF; 100],
            SCOPE_PERSONAL_MESSAGE,
            SuiAddress::ZERO,
        )
        .await
        .unwrap_err();

    assert_debug_snapshot!(result, @r###""[\"Cannot parse signature\"]""###);
}

#[tokio::test]
async fn test_wrong_address() {
    let cluster = FullCluster::new().await.unwrap();
    let (message, signature, _correct_addr) =
        sign_personal_message(&ed25519_keypair(/* seed = */ 1), b"Hello, World!");

    let result = cluster
        .verify(message, signature, SCOPE_PERSONAL_MESSAGE, SuiAddress::ZERO)
        .await
        .unwrap_err();

    assert_debug_snapshot!(result, @r###""[\"Verification failed: Value was not signed by the correct sender: Incorrect signer, expected 0x0000000000000000000000000000000000000000000000000000000000000000, got 0x7fb3efcd05bf58d5151feef3ded095ac5daa018611246e8dd006d0a88c11e719\"]""###);
}

#[tokio::test]
async fn test_wrong_intent_scope() {
    let cluster = FullCluster::new().await.unwrap();
    let (message, signature, addr) =
        sign_personal_message(&ed25519_keypair(/* seed = */ 1), b"Hello, World!");

    let result = cluster
        .verify(message, signature, SCOPE_TRANSACTION_DATA, addr)
        .await
        .unwrap_err();

    assert_debug_snapshot!(result, @r###""[\"Failed to deserialize TransactionData from bytes\"]""###);
}
