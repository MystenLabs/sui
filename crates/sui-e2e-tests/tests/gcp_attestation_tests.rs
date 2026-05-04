// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

#[cfg(msim)]
use fastcrypto_zkp::bn254::zk_login::{JWK, JwkId};
use move_core_types::identifier::Identifier;
use sui_macros::sim_test;
use sui_protocol_config::ProtocolConfig;
use sui_types::SUI_FRAMEWORK_PACKAGE_ID;
use sui_types::effects::TransactionEffectsAPI;
use sui_types::programmable_transaction_builder::ProgrammableTransactionBuilder;
use sui_types::transaction::{CallArg, TransactionData, TransactionKind};
use test_cluster::TestClusterBuilder;

#[cfg(msim)]
const GCP_ISS: &str = "https://confidentialcomputing.googleapis.com";
const TEST_KID: &str = "gcp-test-key-001";

// Pre-generated RSA-2048 test key (base64url-encoded n and e)
#[cfg(msim)]
const KEY_N_B64: &str = "5Q_dKcwT2Bn7jh4qXmNjzsIHtgXRMDAYcOedHYJIZG-qglZg_ZMdmUwp4tF8lL9kXUZ09OvkwCdrH28rm87hA2UookBxHCQL0VIpJnykusCy2pqFb198TQ4xp4GvEgCY823nex6PpV_q-R2efGqMAg6I3VeFb9Fs0-dpDZ_KNZYse3c3y3RromaBK8nXg4dpHEta7i1Em_jaCzXOqwpr0SWJq7J0L6mKCh9jzsETXfzCvQYPG0LC0eZ2cpBViWCZ5iwPN7Wh994my0WWZ5p0zhgNCQsso4e0VlBWii6rjVqZfX-EHMuz5pvzXwlarWy9_L_65SEdM7kgGhsSyg-Hlw";
#[cfg(msim)]
const KEY_E_B64: &str = "AQAB";

// Pre-signed JWT with kid header, far-future expiry (year 2096), signed with key above.
// Payload: iss=GCP, exp=4000000000, iat=1700000000
const E2E_TOKEN: &[u8] = b"eyJhbGciOiJSUzI1NiIsInR5cCI6IkpXVCIsImtpZCI6ImdjcC10ZXN0LWtleS0wMDEifQ.eyJpc3MiOiJodHRwczovL2NvbmZpZGVudGlhbGNvbXB1dGluZy5nb29nbGVhcGlzLmNvbSIsInN1YiI6InRlc3Qtc3ViamVjdCIsImF1ZCI6InRlc3QtYXVkaWVuY2UiLCJleHAiOjQwMDAwMDAwMDAsImlhdCI6MTcwMDAwMDAwMCwiZWF0X25vbmNlIjpbIm5vbmNlMSIsIm5vbmNlMiJdLCJzZWNib290Ijp0cnVlLCJod21vZGVsIjoiR0NQX0FNRF9TRVYiLCJzd25hbWUiOiJDT05GSURFTlRJQUxfU1BBQ0UiLCJkYmdzdGF0IjoiZGlzYWJsZWQtc2luY2UtYm9vdCIsInN3dmVyc2lvbiI6WyIyMzEwMzEiXSwic3VibW9kcyI6eyJjb250YWluZXIiOnsiaW1hZ2VfZGlnZXN0Ijoic2hhMjU2OmFiYzEyMyIsImltYWdlX3JlZmVyZW5jZSI6InVzLWRvY2tlci5wa2cuZGV2L3Byb2plY3QvcmVwby9pbWFnZTpsYXRlc3QiLCJyZXN0YXJ0X3BvbGljeSI6Ik5ldmVyIn19fQ.hAaGvjiPNQrR2h_ogX-hzX3kGsSd_zMAUCZcWgCOprj7dopXZtk7Pspzo-9EW6KoOrC6hfCtifX6xYcxZ3_SnZ_b3OkSaRye7_VtgyF6ymg2SCoHmCj2MQuhqzcbYq0Ob2wi_0TEiI3LrcZbNsSV1xBibDaodZbKptxWVjmD0_QZHdIQLHoTmL0uEPg_X9mQaR2jHsp_CviPfi9eCKdyXj5qeaQqccG5UAsB4mhHs-U-goRwaZ4yI_QVb_kLMC1y0QK1b_C8cpQRW0Ke7iHbTWqXui09ajylMKjqXbCF7C2Hb5dE__G6CZrZjG_Ge9H6vsOQ8CX53Xe2asgIPWeZ_w";

// --- Helpers ---------------------------------------------------------------

fn build_verify_ptb(token: &[u8]) -> ProgrammableTransactionBuilder {
    let mut ptb = ProgrammableTransactionBuilder::new();
    let token_arg = ptb.pure(token.to_vec()).unwrap();
    let kid_arg = ptb.pure(TEST_KID.as_bytes().to_vec()).unwrap();
    let clock_arg = ptb.input(CallArg::CLOCK_IMM).unwrap();
    ptb.programmable_move_call(
        SUI_FRAMEWORK_PACKAGE_ID,
        Identifier::new("gcp_attestation").unwrap(),
        Identifier::new("verify_gcp_attestation").unwrap(),
        vec![],
        vec![token_arg, kid_arg, clock_arg],
    );
    ptb
}

// --- Tests -----------------------------------------------------------------

#[sim_test]
async fn test_gcp_attestation_verifies_on_chain() {
    if sui_simulator::has_mainnet_protocol_config_override() {
        return;
    }

    #[cfg(msim)]
    sui_node::set_gcp_jwk_injector(vec![(
        JwkId {
            iss: GCP_ISS.to_string(),
            kid: TEST_KID.to_string(),
        },
        JWK {
            kty: "RSA".to_string(),
            e: KEY_E_B64.to_string(),
            n: KEY_N_B64.to_string(),
            alg: "RS256".to_string(),
        },
    )]);

    let test_cluster = TestClusterBuilder::new()
        .with_jwk_fetch_interval(std::time::Duration::from_secs(1))
        .build()
        .await;

    test_cluster.trigger_reconfiguration().await;

    let sender = test_cluster.get_address_0();
    let rgp = test_cluster.get_reference_gas_price().await;
    let gas = test_cluster
        .wallet
        .gas_objects(sender)
        .await
        .expect("failed to get gas objects")
        .pop()
        .expect("no gas objects")
        .1
        .compute_object_reference();

    let ptb = build_verify_ptb(E2E_TOKEN);
    let tx_data = TransactionData::new(
        TransactionKind::ProgrammableTransaction(ptb.finish()),
        sender,
        gas,
        10_000_000,
        rgp,
    );
    let tx = test_cluster.wallet.sign_transaction(&tx_data).await;
    test_cluster.execute_transaction(tx).await;
}

#[sim_test]
async fn test_gcp_attestation_rejects_unknown_kid() {
    if sui_simulator::has_mainnet_protocol_config_override() {
        return;
    }

    let test_cluster = TestClusterBuilder::new().build().await;

    let sender = test_cluster.get_address_0();
    let rgp = test_cluster.get_reference_gas_price().await;
    let gas = test_cluster
        .wallet
        .gas_objects(sender)
        .await
        .expect("failed to get gas objects")
        .pop()
        .expect("no gas objects")
        .1
        .compute_object_reference();

    let ptb = build_verify_ptb(E2E_TOKEN);
    let tx_data = TransactionData::new(
        TransactionKind::ProgrammableTransaction(ptb.finish()),
        sender,
        gas,
        10_000_000,
        rgp,
    );
    let tx = test_cluster.wallet.sign_transaction(&tx_data).await;
    let (effects, _) = test_cluster
        .execute_transaction_return_raw_effects(tx)
        .await
        .expect("execution transport error");
    assert!(
        effects.status().is_err(),
        "unknown kid must cause transaction failure"
    );
}

#[sim_test]
async fn test_gcp_attestation_rejected_when_disabled() {
    if sui_simulator::has_mainnet_protocol_config_override() {
        return;
    }

    let _guard = ProtocolConfig::apply_overrides_for_testing(|_, mut config| {
        config.set_enable_gcp_attestation_for_testing(false);
        config
    });

    let test_cluster = TestClusterBuilder::new().build().await;

    let sender = test_cluster.get_address_0();
    let rgp = test_cluster.get_reference_gas_price().await;
    let gas = test_cluster
        .wallet
        .gas_objects(sender)
        .await
        .expect("failed to get gas objects")
        .pop()
        .expect("no gas objects")
        .1
        .compute_object_reference();

    let ptb = build_verify_ptb(E2E_TOKEN);
    let tx_data = TransactionData::new(
        TransactionKind::ProgrammableTransaction(ptb.finish()),
        sender,
        gas,
        10_000_000,
        rgp,
    );
    let tx = test_cluster.wallet.sign_transaction(&tx_data).await;
    let (effects, _) = test_cluster
        .execute_transaction_return_raw_effects(tx)
        .await
        .expect("execution transport error");
    assert!(
        effects.status().is_err(),
        "verify_gcp_attestation must fail when feature is disabled"
    );
}
