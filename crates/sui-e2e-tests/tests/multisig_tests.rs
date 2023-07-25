// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

use sui_core::authority_client::AuthorityAPI;
use sui_json_rpc_types::SuiTransactionBlockEffectsAPI;
use sui_macros::sim_test;
use sui_test_transaction_builder::TestTransactionBuilder;
use sui_types::{
    base_types::SuiAddress,
    error::{SuiError, SuiResult},
    multisig::MultiSigPublicKey,
    multisig_legacy::MultiSigPublicKeyLegacy,
    utils::{keys, make_upgraded_multisig_tx},
};
use test_cluster::TestClusterBuilder;

async fn do_upgraded_multisig_test() -> SuiResult {
    let test_cluster = TestClusterBuilder::new().build().await;
    let tx = make_upgraded_multisig_tx();

    test_cluster
        .authority_aggregator()
        .authority_clients
        .values()
        .next()
        .unwrap()
        .authority_client()
        .handle_transaction(tx)
        .await
        .map(|_| ())
}

#[sim_test]
async fn test_upgraded_multisig_feature_deny() {
    use sui_protocol_config::ProtocolConfig;

    let _guard = ProtocolConfig::apply_overrides_for_testing(|_, mut config| {
        config.set_upgraded_multisig_for_testing(false);
        config
    });

    let err = do_upgraded_multisig_test().await.unwrap_err();

    assert!(matches!(err, SuiError::UnsupportedFeatureError { .. }));
}

#[sim_test]
async fn test_upgraded_multisig_feature_allow() {
    use sui_protocol_config::ProtocolConfig;

    let _guard = ProtocolConfig::apply_overrides_for_testing(|_, mut config| {
        config.set_upgraded_multisig_for_testing(true);
        config
    });

    let res = do_upgraded_multisig_test().await;

    // we didn't make a real transaction with a valid object, but we verify that we pass the
    // feature gate.
    assert!(matches!(res.unwrap_err(), SuiError::UserInputError { .. }));
}

#[sim_test]
async fn test_multisig_e2e() {
    let mut test_cluster = TestClusterBuilder::new().build().await;

    let keys = keys();
    let pk1 = keys[0].public();
    let pk2 = keys[1].public();
    let pk3 = keys[2].public();

    let multisig_pk = MultiSigPublicKey::new(
        vec![pk1.clone(), pk2.clone(), pk3.clone()],
        vec![1, 1, 1],
        2,
    )
    .unwrap();
    let multisig_addr = SuiAddress::from(&multisig_pk);

    let (sender, gas) = test_cluster
        .wallet
        .get_one_gas_object()
        .await
        .unwrap()
        .unwrap();
    let rgp = test_cluster.get_reference_gas_price().await;
    let context = &mut test_cluster.wallet;
    let transfer_to_multisig = context.sign_transaction(
        &TestTransactionBuilder::new(sender, gas, rgp)
            .transfer_sui(Some(20000000000), multisig_addr)
            .build(),
    );
    let resp = context
        .execute_transaction_must_succeed(transfer_to_multisig)
        .await;

    let new_obj = resp
        .effects
        .unwrap()
        .created()
        .first()
        .unwrap()
        .reference
        .to_object_ref();
    // now send it back
    let transfer_from_multisig = TestTransactionBuilder::new(multisig_addr, new_obj, rgp)
        .transfer_sui(Some(1000000), sender)
        .build_and_sign_multisig(multisig_pk, &[&keys[0], &keys[1]]);

    context
        .execute_transaction_must_succeed(transfer_from_multisig)
        .await;
}

#[sim_test]
async fn test_multisig_legacy_e2e() {
    let mut test_cluster = TestClusterBuilder::new().build().await;

    let keys = keys();
    let pk1 = keys[0].public();
    let pk2 = keys[1].public();
    let pk3 = keys[2].public();

    let multisig_pk = MultiSigPublicKeyLegacy::new(
        vec![pk1.clone(), pk2.clone(), pk3.clone()],
        vec![1, 1, 1],
        2,
    )
    .unwrap();
    let multisig_addr = SuiAddress::from(&multisig_pk);

    let (sender, gas) = test_cluster
        .wallet
        .get_one_gas_object()
        .await
        .unwrap()
        .unwrap();
    let rgp = test_cluster.get_reference_gas_price().await;
    let context = &mut test_cluster.wallet;
    let transfer_to_multisig = context.sign_transaction(
        &TestTransactionBuilder::new(sender, gas, rgp)
            .transfer_sui(Some(20000000000), multisig_addr)
            .build(),
    );
    let resp = context
        .execute_transaction_must_succeed(transfer_to_multisig)
        .await;

    let new_obj = resp
        .effects
        .unwrap()
        .created()
        .first()
        .unwrap()
        .reference
        .to_object_ref();
    // now send it back
    let transfer_from_multisig = TestTransactionBuilder::new(multisig_addr, new_obj, rgp)
        .transfer_sui(Some(1000000), sender)
        .build_and_sign_multisig_legacy(multisig_pk, &[&keys[0], &keys[1]]);

    context
        .execute_transaction_must_succeed(transfer_from_multisig)
        .await;
}
