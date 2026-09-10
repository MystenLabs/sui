// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

use fastcrypto::traits::{KeyPair as _, ToFromBytes};
use fastcrypto_pq::mldsa65::MLDSA65KeyPair;
use shared_crypto::intent::{Intent, IntentMessage};
use std::net::SocketAddr;
use sui_core::authority_client::AuthorityAPI;
use sui_macros::sim_test;
use sui_protocol_config::ProtocolConfig;
use sui_test_transaction_builder::TestTransactionBuilder;
use sui_types::base_types::SuiAddress;
use sui_types::crypto::{Signature, SuiKeyPair, get_key_pair};
use sui_types::effects::TransactionEffectsAPI;
use sui_types::error::{SuiErrorKind, SuiResult, UserInputError};
use sui_types::messages_grpc::SubmitTxRequest;
use sui_types::multisig::{MultiSig, MultiSigPublicKey};
use sui_types::multisig_legacy::{MultiSigLegacy, MultiSigPublicKeyLegacy};
use sui_types::signature::GenericSignature;
use sui_types::transaction::{Transaction, TransactionData};
use test_cluster::{TestCluster, TestClusterBuilder};

fn mldsa_keypair() -> SuiKeyPair {
    SuiKeyPair::MLDSA65(MLDSA65KeyPair::generate(&mut rand::thread_rng()))
}

/// Submits to one validator, so `Ok` means that validator's signature
/// verification accepted the transaction.
async fn execute_tx(tx: Transaction, test_cluster: &TestCluster) -> SuiResult {
    test_cluster
        .authority_aggregator()
        .authority_clients
        .values()
        .next()
        .unwrap()
        .authority_client()
        .submit_transaction(
            SubmitTxRequest::new_transaction(tx),
            Some(SocketAddr::new([127, 0, 0, 1].into(), 0)),
        )
        .await
        .map(|_| ())
}

/// Fund the keypair's address and build an unsigned transfer from it.
async fn funded_transfer(test_cluster: &TestCluster, kp: &SuiKeyPair) -> TransactionData {
    let sender = SuiAddress::from(&kp.public());
    let rgp = test_cluster.get_reference_gas_price().await;
    let gas = test_cluster
        .fund_address_and_return_gas(rgp, Some(20000000000), sender)
        .await;
    TestTransactionBuilder::new(sender, gas, rgp)
        .transfer_sui(None, SuiAddress::ZERO)
        .build()
}

fn sign(tx_data: TransactionData, kp: &SuiKeyPair) -> Transaction {
    let intent_msg = IntentMessage::new(Intent::sui_transaction(), tx_data);
    let sig = Signature::new_secure(&intent_msg, kp);
    Transaction::from_generic_sig_data(intent_msg.value, vec![GenericSignature::Signature(sig)])
}

/// Tests of the enabled path turn the flag on explicitly rather than depending
/// on which protocol versions carry it; `test_mldsa65_feature_deny` pins it off.
fn enable_mldsa() -> sui_protocol_config::OverrideGuard {
    ProtocolConfig::apply_overrides_for_testing(|_, mut config| {
        config.set_mldsa65_auth_for_testing(true);
        config
    })
}

fn enable_mldsa_in_multisig() -> sui_protocol_config::OverrideGuard {
    ProtocolConfig::apply_overrides_for_testing(|_, mut config| {
        config.set_mldsa65_auth_for_testing(true);
        config.set_accept_mldsa65_in_multisig_for_testing(true);
        config
    })
}

#[sim_test]
async fn test_mldsa65_transfer_executes() {
    let _guard = enable_mldsa();
    let test_cluster = TestClusterBuilder::new().build().await;
    let kp = mldsa_keypair();
    let tx_data = funded_transfer(&test_cluster, &kp).await;
    let (effects, _) = test_cluster
        .execute_transaction_return_raw_effects(sign(tx_data, &kp))
        .await
        .unwrap();
    assert!(effects.status().is_ok(), "{:?}", effects.status());
}

#[sim_test]
async fn test_mldsa65_feature_deny() {
    // With the flag pinned off, the transaction is refused as Unsupported
    // before signature verification runs.
    let _guard = ProtocolConfig::apply_overrides_for_testing(|_, mut config| {
        config.set_mldsa65_auth_for_testing(false);
        config
    });
    let test_cluster = TestClusterBuilder::new().build().await;
    let kp = mldsa_keypair();
    let tx_data = funded_transfer(&test_cluster, &kp).await;
    let err = execute_tx(sign(tx_data, &kp), &test_cluster)
        .await
        .unwrap_err();
    assert!(matches!(
        err.as_inner(),
        SuiErrorKind::UserInputError {
            error: UserInputError::Unsupported(..)
        }
    ));
}

#[sim_test]
async fn test_mldsa65_wrong_signer_fails() {
    let _guard = enable_mldsa();
    let test_cluster = TestClusterBuilder::new().build().await;
    let kp = mldsa_keypair();
    let other = mldsa_keypair();
    let tx_data = funded_transfer(&test_cluster, &kp).await;
    // Signed by a different ML-DSA key than the sender's: past the feature
    // gate, rejected at signature verification.
    let err = execute_tx(sign(tx_data, &other), &test_cluster)
        .await
        .unwrap_err();
    assert!(
        matches!(err.as_inner(), SuiErrorKind::SignerSignatureAbsent { .. }),
        "must fail verification, not the feature gate: {err:?}"
    );
}

#[sim_test]
async fn test_mldsa65_tampered_signature_fails() {
    let _guard = enable_mldsa();
    let test_cluster = TestClusterBuilder::new().build().await;
    let kp = mldsa_keypair();
    let tx_data = funded_transfer(&test_cluster, &kp).await;
    let intent_msg = IntentMessage::new(Intent::sui_transaction(), tx_data);
    let sig = Signature::new_secure(&intent_msg, &kp);

    // Flip one byte inside the signature body. The envelope still parses
    // (flag, length and pk intact), so the failure is verification proper.
    let mut bytes = sig.as_ref().to_vec();
    bytes[100] ^= 0x01;
    let tampered = GenericSignature::from_bytes(&bytes).unwrap();
    let tx = Transaction::from_generic_sig_data(intent_msg.value, vec![tampered]);
    let err = execute_tx(tx, &test_cluster).await.unwrap_err();
    assert!(
        matches!(err.as_inner(), SuiErrorKind::InvalidSignature { .. }),
        "must fail verification, not the feature gate: {err:?}"
    );
}

/// A funded committee of one Ed25519 and one ML-DSA-65 member (weight 1 each)
/// and a transfer from it, ready to be signed by any subset of the members.
struct HybridCommittee {
    ed_kp: SuiKeyPair,
    mldsa_kp: SuiKeyPair,
    multisig_pk: MultiSigPublicKey,
    legacy_pk: MultiSigPublicKeyLegacy,
    intent_msg: IntentMessage<TransactionData>,
}

impl HybridCommittee {
    async fn funded(test_cluster: &TestCluster, threshold: u16) -> Self {
        let ed_kp = SuiKeyPair::Ed25519(get_key_pair().1);
        let mldsa_kp = mldsa_keypair();
        let pks = vec![ed_kp.public(), mldsa_kp.public()];
        let multisig_pk = MultiSigPublicKey::new(pks.clone(), vec![1, 1], threshold).unwrap();
        let legacy_pk = MultiSigPublicKeyLegacy::new(pks, vec![1, 1], threshold).unwrap();
        let sender = SuiAddress::from(&multisig_pk);

        let rgp = test_cluster.get_reference_gas_price().await;
        let gas = test_cluster
            .fund_address_and_return_gas(rgp, Some(20000000000), sender)
            .await;
        let tx_data = TestTransactionBuilder::new(sender, gas, rgp)
            .transfer_sui(None, SuiAddress::ZERO)
            .build();
        Self {
            ed_kp,
            mldsa_kp,
            multisig_pk,
            legacy_pk,
            intent_msg: IntentMessage::new(Intent::sui_transaction(), tx_data),
        }
    }

    /// The transfer signed by `signers`, in the upgraded and the legacy multisig format.
    fn signed_by(&self, signers: &[&SuiKeyPair]) -> [Transaction; 2] {
        let sigs: Vec<GenericSignature> = signers
            .iter()
            .map(|kp| Signature::new_secure(&self.intent_msg, *kp).into())
            .collect();
        [
            GenericSignature::MultiSig(
                MultiSig::combine(sigs.clone(), self.multisig_pk.clone()).unwrap(),
            ),
            GenericSignature::MultiSigLegacy(
                MultiSigLegacy::combine(sigs, self.legacy_pk.clone()).unwrap(),
            ),
        ]
        .map(|generic| {
            Transaction::from_generic_sig_data(self.intent_msg.value.clone(), vec![generic])
        })
    }
}

#[sim_test]
async fn test_mldsa65_multisig_member_denied() {
    // `mldsa65_auth` alone does not admit an ML-DSA member's signature inside
    // a multisig: that is `accept_mldsa65_in_multisig`, off here.
    let _guard = enable_mldsa();
    let test_cluster = TestClusterBuilder::new().build().await;
    let committee = HybridCommittee::funded(&test_cluster, 1).await;
    for tx in committee.signed_by(&[&committee.mldsa_kp]) {
        let err = execute_tx(tx, &test_cluster).await.unwrap_err();
        assert!(
            err.to_string()
                .contains("ML-DSA-65 sig not supported inside multisig"),
            "{err:?}"
        );
    }
}

#[sim_test]
async fn test_mldsa65_multisig_hybrid_executes() {
    // Threshold 2 of two weight-1 members: both the Ed25519 and the ML-DSA
    // signatures are required, and with the flag on the transfer executes.
    let _guard = enable_mldsa_in_multisig();
    let test_cluster = TestClusterBuilder::new().build().await;
    let committee = HybridCommittee::funded(&test_cluster, 2).await;
    let [upgraded, _legacy] = committee.signed_by(&[&committee.ed_kp, &committee.mldsa_kp]);
    let (effects, _) = test_cluster
        .execute_transaction_return_raw_effects(upgraded)
        .await
        .unwrap();
    assert!(effects.status().is_ok(), "{:?}", effects.status());
}
