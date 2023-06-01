// Copyright (c) 2021, Facebook, Inc. and its affiliates
// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

use sui_types::error::{SuiError, SuiResult};
use sui_types::utils::make_zklogin_tx;
use test_utils::authority::{spawn_test_authorities, test_authority_configs_with_objects};

use sui_core::{authority_aggregator::AuthorityAggregatorBuilder, authority_client::AuthorityAPI};
use sui_macros::sim_test;
use sui_types::object::generate_test_gas_objects;

async fn do_zklogin_test() -> SuiResult {
    let gas_objects = generate_test_gas_objects();

    // Get the authority configs and spawn them. Note that it is important to not drop
    // the handles (or the authorities will stop).
    let (config, _) = test_authority_configs_with_objects(gas_objects);
    let _handles = spawn_test_authorities(&config).await;

    let (_, tx, _) = make_zklogin_tx();

    let (_agg, clients) = AuthorityAggregatorBuilder::from_network_config(&config)
        .build()
        .unwrap();

    clients
        .values()
        .next()
        .unwrap()
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
