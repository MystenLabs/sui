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
use test_cluster::{TestCluster, TestClusterBuilder};

#[cfg(msim)]
const GCP_ISS: &str = "https://confidentialcomputing.googleapis.com";
const TEST_KID: &str = "gcp-test-key-001";

// Pre-generated RSA-2048 test key (base64url-encoded n and e).
// e = AQAB = 65537 (valid exponent).
#[cfg(msim)]
const KEY_N_B64: &str = "z09gNzD2KFKzXDBsXBpwomlx7zXXBWZIokl5A_LIYB-WZwnTWlpMM55uUV9JG-PN5K_--qfnR7pgEPaqkTzQ4GRYnluQuCGuOSQnJF8cjFOCF_gEW_iVz6387cE-dm-5Skq-BIPVJr9xVUTZ2R-hKJJElwBFpbXBViFBLMocJxvctbwHmgrkje5HH1JUboW_ruNIlWRJAwgBHNzS2l087l66njwmx85j_vnNI259pZe3RBKKOhTiol7NjP0U2b9c6DZOXt0mhzwZjWilUZ__ycS9ldnl8ebABbmOEd3aYULxZYK4QG12AVfe9fEAZ--Re9zuINO9yY8XFK0EEfQSPw";
#[cfg(msim)]
const KEY_E_B64: &str = "AQAB";

// Pre-signed JWT with kid header, iat before msim chain start (~2022-01-03),
// far-future expiry (year 2096), signed with key above.
const E2E_TOKEN: &[u8] = b"eyJhbGciOiJSUzI1NiIsInR5cCI6IkpXVCIsImtpZCI6ImdjcC10ZXN0LWtleS0wMDEifQ.eyJpc3MiOiJodHRwczovL2NvbmZpZGVudGlhbGNvbXB1dGluZy5nb29nbGVhcGlzLmNvbSIsInN1YiI6InRlc3Qtc3ViamVjdCIsImF1ZCI6InRlc3QtYXVkaWVuY2UiLCJleHAiOjQwMDAwMDAwMDAsImlhdCI6MTYwMDAwMDAwMCwiZWF0X25vbmNlIjpbIm5vbmNlMSIsIm5vbmNlMiJdLCJzZWNib290Ijp0cnVlLCJod21vZGVsIjoiR0NQX0FNRF9TRVYiLCJzd25hbWUiOiJDT05GSURFTlRJQUxfU1BBQ0UiLCJkYmdzdGF0IjoiZGlzYWJsZWQtc2luY2UtYm9vdCIsInN3dmVyc2lvbiI6WyIyMzEwMzEiXSwic3VibW9kcyI6eyJjb250YWluZXIiOnsiaW1hZ2VfZGlnZXN0Ijoic2hhMjU2OmFiYzEyMyIsImltYWdlX3JlZmVyZW5jZSI6InVzLWRvY2tlci5wa2cuZGV2L3Byb2plY3QvcmVwby9pbWFnZTpsYXRlc3QiLCJyZXN0YXJ0X3BvbGljeSI6Ik5ldmVyIn19fQ.cxeSqrbz8s34Dv5ckeHy3hLD3lcpXONFwO8DErW0M2ccHEDiEdRlh0sSopMPg5zsUghZ0RhjvzMy03farg_-JIetX9cxRPpyU2FUkwZadIPWGnmo7M1I3aa9ADNlnhIubCTnb3AVm0xDjkf_WuNr1YfWW5KjS9dmmDPMlUO0NEghfSaBtJNrYvSd0JzclnavF_y5CyGhDkVNSnmm0g0QGpLmtu_NKz-g0Io9v5viYYibOR4Uqe2osUYhpYCh26_k4pCJokD5ySyC7MtgPzgpGR6lNTxY7X3skqRzr13A2SUHy-L6L0jZwHhv9AZScOcHUd6l0ufSlbZNAzw_8uJKyw";

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

async fn sign_verify_tx(test_cluster: &TestCluster, token: &[u8]) -> sui_types::transaction::Transaction {
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

    let ptb = build_verify_ptb(token);
    let tx_data = TransactionData::new(
        TransactionKind::ProgrammableTransaction(ptb.finish()),
        sender,
        gas,
        10_000_000,
        rgp,
    );
    test_cluster.wallet.sign_transaction(&tx_data).await
}

#[cfg(msim)]
fn inject_test_gcp_jwk() {
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
}

#[cfg(msim)]
fn clear_gcp_jwk_injector() {
    sui_node::set_gcp_jwk_injector(vec![]);
}

// Happy path needs msim GCP JWK injection; skip under plain cargo test.
#[cfg(msim)]
#[sim_test]
async fn test_gcp_attestation_verifies_on_chain() {
    if sui_simulator::has_mainnet_protocol_config_override() {
        return;
    }

    inject_test_gcp_jwk();

    let test_cluster = TestClusterBuilder::new()
        .with_jwk_fetch_interval(std::time::Duration::from_secs(1))
        .build()
        .await;

    // Wait until the injected GCP JWK is committed into AuthenticatorState.
    // Do not rely on a single trigger_reconfiguration() — that raced in CI.
    test_cluster
        .wait_for_authenticator_state_update_for_providers(&[JwkId {
            iss: GCP_ISS.to_string(),
            kid: TEST_KID.to_string(),
        }])
        .await;

    let tx = sign_verify_tx(&test_cluster, E2E_TOKEN).await;
    test_cluster.execute_transaction(tx).await;
}

#[sim_test]
async fn test_gcp_attestation_rejects_unknown_kid() {
    if sui_simulator::has_mainnet_protocol_config_override() {
        return;
    }

    #[cfg(msim)]
    clear_gcp_jwk_injector();

    let test_cluster = TestClusterBuilder::new().build().await;

    let tx = sign_verify_tx(&test_cluster, E2E_TOKEN).await;
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

    #[cfg(msim)]
    clear_gcp_jwk_injector();

    let test_cluster = TestClusterBuilder::new().build().await;

    let tx = sign_verify_tx(&test_cluster, E2E_TOKEN).await;
    let (effects, _) = test_cluster
        .execute_transaction_return_raw_effects(tx)
        .await
        .expect("execution transport error");
    assert!(
        effects.status().is_err(),
        "verify_gcp_attestation must fail when feature is disabled"
    );
}
