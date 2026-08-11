// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

use itertools::Itertools;
use prost_types::value::Kind;
use sui_macros::sim_test;
use sui_protocol_config::ProtocolConfig;
use sui_rpc::field::FieldMask;
use sui_rpc::field::FieldMaskUtil;
use sui_rpc::proto::sui::rpc::v2::GetEpochRequest;
use sui_rpc::proto::sui::rpc::v2::ledger_service_client::LedgerServiceClient;
use test_cluster::TestClusterBuilder;

#[sim_test]
async fn get_epoch() {
    let test_cluster = TestClusterBuilder::new()
        .with_num_validators(1)
        .build()
        .await;

    let mut client = LedgerServiceClient::connect(test_cluster.rpc_url().to_owned())
        .await
        .unwrap();

    let latest_epoch = client
        .get_epoch(GetEpochRequest::latest().with_read_mask(FieldMask::from_str("*")))
        .await
        .unwrap()
        .into_inner()
        .epoch
        .unwrap();

    let epoch_0 = client
        .get_epoch(GetEpochRequest::new(0).with_read_mask(FieldMask::from_str("*")))
        .await
        .unwrap()
        .into_inner()
        .epoch
        .unwrap();

    assert_eq!(latest_epoch.committee, epoch_0.committee);

    // ensure we can convert proto committee type to sdk_types committee
    sui_sdk_types::ValidatorCommittee::try_from(&latest_epoch.committee.unwrap()).unwrap();

    assert_eq!(epoch_0.epoch, Some(0));
    assert_eq!(epoch_0.first_checkpoint, Some(0));

    //Ensure that fetching the system state for the epoch works
    let epoch = client
        .get_epoch(
            GetEpochRequest::latest().with_read_mask(FieldMask::from_paths(["system_state"])),
        )
        .await
        .unwrap()
        .into_inner()
        .epoch
        .unwrap();
    assert!(epoch.system_state.is_some());

    let status = client
        .get_epoch(
            GetEpochRequest::new(latest_epoch.epoch.unwrap() + 1000)
                .with_read_mask(FieldMask::from_str("*")),
        )
        .await
        .unwrap_err();
    assert_eq!(status.code(), tonic::Code::NotFound);
}

#[sim_test]
async fn get_epoch_protocol_config_exposes_gasless_allowlist() {
    // The allowlist is a non-scalar `Vec<(String, u64)>`. The render path should expose it as
    // a structured `ListValue` under `configs` and as a JSON-stringified copy under the legacy
    // `attributes` map.
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

    let mut client = LedgerServiceClient::connect(test_cluster.rpc_url().to_owned())
        .await
        .unwrap();

    let epoch = client
        .get_epoch(
            GetEpochRequest::latest().with_read_mask(FieldMask::from_paths(["protocol_config"])),
        )
        .await
        .unwrap()
        .into_inner()
        .epoch
        .unwrap();

    let protocol_config = epoch
        .protocol_config
        .expect("protocol_config should be present");

    let key = "gasless_allowed_token_types";

    // `configs` exposes the value losslessly as a `prost_types::Value`. The expected shape is a
    // ListValue of inner ListValues of [coin_type_string, amount_string] — u64 amounts render as
    // strings to preserve JS precision.
    let configs_value = protocol_config
        .configs
        .get(key)
        .unwrap_or_else(|| panic!("`{key}` should be present in `configs`"));
    let Some(Kind::ListValue(outer)) = &configs_value.kind else {
        panic!("expected `configs.{key}` to be a ListValue, got {configs_value:?}");
    };
    assert_eq!(outer.values.len(), allowlist.len());
    for (entry_value, (expected_coin_type, expected_amount)) in
        outer.values.iter().zip_eq(allowlist.iter())
    {
        let Some(Kind::ListValue(entry)) = &entry_value.kind else {
            panic!("expected each entry to be a ListValue, got {entry_value:?}");
        };
        assert_eq!(entry.values.len(), 2, "each entry is (coin_type, amount)");

        let Some(Kind::StringValue(coin_type)) = &entry.values[0].kind else {
            panic!(
                "expected coin_type as StringValue, got {:?}",
                entry.values[0]
            );
        };
        assert_eq!(coin_type, expected_coin_type);

        let Some(Kind::StringValue(amount)) = &entry.values[1].kind else {
            panic!(
                "expected amount as StringValue (precision-safe u64), got {:?}",
                entry.values[1],
            );
        };
        assert_eq!(amount, &expected_amount.to_string());
    }

    // `attributes` is the legacy stringified view. Complex types are emitted as JSON strings.
    let attribute = protocol_config
        .attributes
        .get(key)
        .unwrap_or_else(|| panic!("`{key}` should be present in `attributes`"));
    let parsed: serde_json::Value =
        serde_json::from_str(attribute).expect("attribute should be valid JSON");
    let expected_json =
        serde_json::json!([["0xa::usdc::USDC", "10000"], ["0xb::usdt::USDT", "0"],]);
    assert_eq!(parsed, expected_json);
}
