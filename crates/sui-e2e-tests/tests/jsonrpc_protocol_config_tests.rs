// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

use sui_macros::sim_test;
use sui_protocol_config::ProtocolConfig;
use test_cluster::TestClusterBuilder;

#[sim_test]
async fn get_protocol_config_exposes_gasless_allowlist_in_configs() {
    // The allowlist is a non-scalar `Vec<(String, u64)>`. The new `configs` field on the
    // JSON-RPC `ProtocolConfigResponse` should expose it as structured JSON, while the legacy
    // `attributes` map omits it (its value type only supports scalar primitives).
    let allowlist = vec![
        ("0xa::usdc::USDC".to_string(), 10_000u64),
        ("0xb::usdt::USDT".to_string(), 0u64),
    ];
    let allowlist_for_override = allowlist.clone();
    let _guard = ProtocolConfig::apply_overrides_for_testing(move |_, mut cfg| {
        cfg.set_gasless_allowed_token_types_for_testing(allowlist_for_override.clone());
        cfg
    });

    let test_cluster = TestClusterBuilder::new()
        .with_num_validators(1)
        .build()
        .await;

    #[allow(deprecated)]
    let client = test_cluster.sui_client();

    let response = client
        .read_api()
        .get_protocol_config(None)
        .await
        .expect("getProtocolConfig should succeed");

    let key = "gasless_allowed_token_types";

    // The `configs` map carries the full, lossless rendering. u64 amounts render as strings to
    // preserve JS precision, so each entry is `[coin_type_string, amount_string]`.
    let value = response
        .configs
        .get(key)
        .unwrap_or_else(|| panic!("`{key}` should be present in `configs`"));
    let expected = serde_json::json!([["0xa::usdc::USDC", "10000"], ["0xb::usdt::USDT", "0"],]);
    assert_eq!(value, &expected);

    // The legacy `attributes` map is typed against scalar primitives only, so non-scalar fields
    // like the allowlist never appear there.
    assert!(
        !response.attributes.contains_key(key),
        "non-scalar `{key}` should not surface in legacy `attributes`",
    );
}
