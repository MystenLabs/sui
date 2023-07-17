// Copyright (c) 2021, Facebook, Inc. and its affiliates
// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

use sui_core::authority_client::AuthorityAPI;
use sui_macros::sim_test;
use sui_types::error::{SuiError, SuiResult};
use sui_types::utils::make_zklogin_tx;
use test_cluster::TestClusterBuilder;

async fn do_zklogin_test() -> SuiResult {
    let test_cluster = TestClusterBuilder::new().build().await;
    let (_, tx, _) = make_zklogin_tx();

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
async fn test_zklogin_feature_deny() {
    use sui_protocol_config::ProtocolConfig;

    let _guard = ProtocolConfig::apply_overrides_for_testing(|_, mut config| {
        config.set_zklogin_auth(false);
        config
    });

    let err = do_zklogin_test().await.unwrap_err();

    assert!(matches!(err, SuiError::UnsupportedFeatureError { .. }));
}

#[sim_test]
async fn test_zklogin_feature_allow() {
    use sui_protocol_config::ProtocolConfig;

    let _guard = ProtocolConfig::apply_overrides_for_testing(|_, mut config| {
        config.set_zklogin_auth(true);
        config
    });

    let err = do_zklogin_test().await.unwrap_err();

    // we didn't make a real transaction with a valid object, but we verify that we pass the
    // feature gate.
    assert!(matches!(err, SuiError::UserInputError { .. }));
}
