// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

use insta::assert_debug_snapshot;
use move_core_types::language_storage::StructTag;
use serde::Deserialize;
use serde_json::json;
use sui_indexer_alt_e2e_tests::{
    FullCluster,
    coin_registry::{self, LegacyCoinOutputs},
    find,
};
use sui_types::{
    Identifier, SUI_COIN_REGISTRY_ADDRESS,
    base_types::{ObjectRef, SequenceNumber, SuiAddress},
    coin::{CoinMetadata, TreasuryCap},
    deny_list_v2::DenyCapV2,
    effects::TransactionEffectsAPI,
};

#[derive(Deserialize, Clone, Eq, PartialEq, Debug)]
#[serde(rename_all = "camelCase")]
struct CoinMetadataResponse {
    name: String,
    decimals: u8,
    description: String,
    icon_url: Option<String>,
    symbol: String,
}

#[tokio::test]
async fn test_sui() {
    // SUI coin is available from genesis, no need to publish
    let mut cluster = FullCluster::new().await.unwrap();
    cluster.create_checkpoint().await;

    let metadata = query_metadata(&cluster, "0x2::sui::SUI").await;
    assert_debug_snapshot!(metadata, @r###"
    CoinMetadataResponse {
        name: "Sui",
        decimals: 9,
        description: "",
        icon_url: None,
        symbol: "SUI",
    }
    "###);
}

#[tokio::test]
async fn test_fixed_supply() {
    let mut cluster = FullCluster::new().await.unwrap();
    let (a, kp, fx) = coin_registry::publish(&mut cluster, "fixed_supply").await;
    let package = find::immutable(&fx).unwrap().0;
    let currency = find::address_owned_by(&fx, SUI_COIN_REGISTRY_ADDRESS.into()).unwrap();
    let gas = fx.gas_object().0;

    coin_registry::finalize(&mut cluster, a, &kp, package, "fixed", currency, gas).await;
    cluster.create_checkpoint().await;

    let metadata = query_metadata(&cluster, &format!("{package}::fixed::FIXED")).await;
    assert_debug_snapshot!(metadata, @r###"
    CoinMetadataResponse {
        name: "Fixed",
        decimals: 2,
        description: "A fake fixed-supply coin for test purposes",
        icon_url: Some(
            "https://example.com/fake.png",
        ),
        symbol: "FIXED",
    }
    "###);
}

#[tokio::test]
async fn test_dynamic() {
    let mut cluster = FullCluster::new().await.unwrap();
    let (sender, kp, fx) = coin_registry::publish(&mut cluster, "dynamic").await;
    let package = find::immutable(&fx).unwrap().0;
    let gas = fx.gas_object().0;

    // Create a dynamic currency
    let coin_type = StructTag {
        address: package.into(),
        module: Identifier::new("dynamic").unwrap(),
        name: Identifier::new("Dynamic").unwrap(),
        type_params: vec![],
    };

    coin_registry::create_dynamic_currency(&mut cluster, sender, &kp, coin_type, gas).await;
    cluster.create_checkpoint().await;

    let metadata = query_metadata(&cluster, &format!("{package}::dynamic::Dynamic")).await;
    assert_debug_snapshot!(metadata, @r###"
    CoinMetadataResponse {
        name: "Dynamic",
        decimals: 2,
        description: "A fake dynamic coin for test purposes",
        icon_url: Some(
            "https://example.com/dynamic.png",
        ),
        symbol: "DYNAMIC",
    }
    "###);
}

#[tokio::test]
async fn test_burn_only() {
    let mut cluster = FullCluster::new().await.unwrap();
    let (sender, kp, fx) = coin_registry::publish(&mut cluster, "burn_only").await;
    let package = find::immutable(&fx).unwrap().0;
    let currency = find::address_owned_by(&fx, SUI_COIN_REGISTRY_ADDRESS.into()).unwrap();
    let gas = fx.gas_object().0;

    coin_registry::finalize(&mut cluster, sender, &kp, package, "burn", currency, gas).await;
    cluster.create_checkpoint().await;

    let metadata = query_metadata(&cluster, &format!("{package}::burn::BURN")).await;
    assert_debug_snapshot!(metadata, @r###"
    CoinMetadataResponse {
        name: "Burn",
        decimals: 2,
        description: "A fake burn-only coin for test purposes",
        icon_url: Some(
            "https://example.com/fake.png",
        ),
        symbol: "BURN",
    }
    "###);
}

#[tokio::test]
async fn test_unknown() {
    let mut cluster = FullCluster::new().await.unwrap();
    let (sender, kp, fx) = coin_registry::publish(&mut cluster, "unknown").await;
    let package = find::immutable(&fx).unwrap().0;
    let currency = find::address_owned_by(&fx, SUI_COIN_REGISTRY_ADDRESS.into()).unwrap();
    let gas = fx.gas_object().0;

    coin_registry::finalize(&mut cluster, sender, &kp, package, "unknown", currency, gas).await;
    cluster.create_checkpoint().await;

    let metadata = query_metadata(&cluster, &format!("{package}::unknown::UNKNOWN")).await;
    assert_debug_snapshot!(metadata, @r###"
    CoinMetadataResponse {
        name: "Unknown",
        decimals: 2,
        description: "A fake unknown treasury coin for test purposes",
        icon_url: Some(
            "https://example.com/unknown.png",
        ),
        symbol: "UNKNOWN",
    }
    "###);
}

#[tokio::test]
async fn test_legacy() {
    let mut cluster = FullCluster::new().await.unwrap();
    let (sender, kp, fx) = coin_registry::publish(&mut cluster, "legacy").await;

    cluster.create_checkpoint().await;
    let outputs = query_owned_outputs(&cluster, sender).await;
    let gas = fx.gas_object().0;

    let metadata = query_metadata(&cluster, &outputs.coin_type.to_canonical_string(true)).await;
    assert_debug_snapshot!(metadata, @r###"
    CoinMetadataResponse {
        name: "Legacy",
        decimals: 2,
        description: "A fake legacy coin for test purposes",
        icon_url: Some(
            "https://example.com/legacy.png",
        ),
        symbol: "LEGACY",
    }
    "###);

    // Migrate the legacy coin to the coin registry
    coin_registry::migrate(&mut cluster, sender, &kp, &outputs, gas).await;

    cluster.create_checkpoint().await;
    let outputs = query_owned_outputs(&cluster, sender).await;

    // RPC output should be the same after the migration
    let migrated = query_metadata(&cluster, &outputs.coin_type.to_canonical_string(true)).await;
    assert_eq!(metadata, migrated);
}

#[tokio::test]
async fn test_regulated() {
    let mut cluster = FullCluster::new().await.unwrap();
    let (a, kp, fx) = coin_registry::publish(&mut cluster, "regulated").await;
    let package = find::immutable(&fx).unwrap().0;
    let currency = find::address_owned_by(&fx, SUI_COIN_REGISTRY_ADDRESS.into()).unwrap();
    let gas = fx.gas_object().0;

    coin_registry::finalize(&mut cluster, a, &kp, package, "regulated", currency, gas).await;
    cluster.create_checkpoint().await;

    let metadata = query_metadata(&cluster, &format!("{package}::regulated::REGULATED")).await;
    assert_debug_snapshot!(metadata, @r###"
    CoinMetadataResponse {
        name: "Regulated",
        decimals: 2,
        description: "A fake regulated coin for test purposes",
        icon_url: Some(
            "https://example.com/regulated.png",
        ),
        symbol: "REGULATED",
    }
    "###);
}

/// Run a JSONRPC query to fetch the coin metadata for `coin_type` from `cluster`.
async fn query_metadata(cluster: &FullCluster, coin_type: &str) -> CoinMetadataResponse {
    let client = reqwest::Client::new();
    let url = cluster.jsonrpc_url();

    let response: serde_json::Value = client
        .post(url.as_str())
        .json(&json!({
            "jsonrpc": "2.0",
            "method": "suix_getCoinMetadata",
            "params": [coin_type],
            "id": 1
        }))
        .send()
        .await
        .unwrap()
        .json()
        .await
        .unwrap();

    let metadata = response.get("result").unwrap();
    serde_json::from_value(metadata.clone()).unwrap()
}

async fn query_owned_outputs(cluster: &FullCluster, owner: SuiAddress) -> LegacyCoinOutputs {
    let objects = query_objects(cluster, owner).await;

    let (type_, treasury) = objects
        .iter()
        .find_map(|(type_, obj)| {
            TreasuryCap::is_treasury_with_coin_type(type_).map(|type_| (type_.clone(), *obj))
        })
        .unwrap();

    let metadata = objects
        .iter()
        .find_map(|(type_, obj)| CoinMetadata::is_coin_metadata(type_).then_some(*obj));

    let deny = objects
        .iter()
        .find_map(|(type_, obj)| DenyCapV2::is_deny_cap_v2(type_).then_some(*obj));

    LegacyCoinOutputs {
        coin_type: type_,
        treasury,
        metadata,
        deny,
    }
}

/// Query all the owned objects of `owner` using the JSON-RPC API on `cluster`.
async fn query_objects(cluster: &FullCluster, owner: SuiAddress) -> Vec<(StructTag, ObjectRef)> {
    #[derive(Deserialize)]
    #[serde(rename_all = "camelCase")]
    struct Response {
        result: Page,
    }

    #[derive(Deserialize)]
    #[serde(rename_all = "camelCase")]
    struct Page {
        data: Vec<Object>,
        next_cursor: Option<String>,
        has_next_page: bool,
    }

    #[derive(Deserialize)]
    #[serde(rename_all = "camelCase")]
    struct Object {
        data: Data,
    }

    #[derive(Deserialize)]
    #[serde(rename_all = "camelCase")]
    struct Data {
        object_id: String,
        version: String,
        digest: String,
        #[serde(rename = "type")]
        type_: String,
    }

    let client = reqwest::Client::new();
    let url = cluster.jsonrpc_url();

    let mut objects = vec![];
    let mut cursor = None;

    loop {
        let response: Response = client
            .post(url.as_str())
            .json(&json!({
                "jsonrpc": "2.0",
                "method": "suix_getOwnedObjects",
                "params": [
                    owner.to_string(),
                    {
                        "filter": null,
                        "options": { "showType": true },
                    },
                    cursor,
                    50,
                ],
                "id": 1
            }))
            .send()
            .await
            .unwrap()
            .json()
            .await
            .unwrap();

        for object in response.result.data {
            let object_id = object.data.object_id.parse().unwrap();
            let version = SequenceNumber::from_u64(object.data.version.parse::<u64>().unwrap());
            let digest = object.data.digest.parse().unwrap();
            let type_: StructTag = object.data.type_.parse().unwrap();

            objects.push((type_, (object_id, version, digest)));
        }

        if !response.result.has_next_page {
            break;
        }

        cursor = response.result.next_cursor;
    }

    objects
}
