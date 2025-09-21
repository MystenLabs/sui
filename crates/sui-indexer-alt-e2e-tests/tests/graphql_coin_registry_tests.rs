// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

use insta::assert_debug_snapshot;
use move_core_types::language_storage::StructTag;
use serde::Deserialize;
use serde_json::json;
use sui_indexer_alt_e2e_tests::{
    coin_registry::{self, LegacyCoinOutputs},
    find, FullCluster,
};
use sui_types::{
    base_types::{ObjectID, ObjectRef, SequenceNumber, SuiAddress},
    coin::{CoinMetadata, TreasuryCap},
    deny_list_v2::DenyCapV2,
    digests::ObjectDigest,
    effects::TransactionEffectsAPI,
    object::Owner,
    Identifier, SUI_COIN_REGISTRY_ADDRESS,
};

const METADATA_QUERY: &str = r#"
query GetCoinMetadata($coinType: String!) {
    coinMetadata(coinType: $coinType) {
        name
        decimals
        description
        iconUrl
        supply
        supplyState
        symbol
        regulatedState
        allowGlobalPause
    }
}
"#;

const OBJECTS_QUERY: &str = r#"
query GetObjects($owner: SuiAddress!, $after: String) {
    address(address: $owner) {
        objects(after: $after) {
            pageInfo {
                hasNextPage
                endCursor
            }
            nodes {
                address
                version
                digest
                contents { type { repr } }
            }
        }
    }
}
"#;

#[derive(Deserialize, Clone, Eq, PartialEq, Debug)]
#[serde(rename_all = "camelCase")]
struct CoinMetadataResponse {
    name: String,
    decimals: u8,
    description: String,
    icon_url: Option<String>,
    supply: Option<String>,
    symbol: String,
    supply_state: Option<String>,
    regulated_state: Option<String>,
    allow_global_pause: Option<bool>,
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
        supply: Some(
            "10000000000000000000",
        ),
        symbol: "SUI",
        supply_state: Some(
            "FIXED",
        ),
        regulated_state: Some(
            "UNREGULATED",
        ),
        allow_global_pause: Some(
            false,
        ),
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
        supply: Some(
            "1000000000",
        ),
        symbol: "FIXED",
        supply_state: Some(
            "FIXED",
        ),
        regulated_state: Some(
            "UNREGULATED",
        ),
        allow_global_pause: Some(
            false,
        ),
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
        supply: Some(
            "1000000000",
        ),
        symbol: "DYNAMIC",
        supply_state: Some(
            "FIXED",
        ),
        regulated_state: Some(
            "UNREGULATED",
        ),
        allow_global_pause: Some(
            false,
        ),
    }
    "###);
}

#[tokio::test]
async fn test_burn_only() {
    let mut cluster = FullCluster::new().await.unwrap();
    let (sender, kp, fx) = coin_registry::publish(&mut cluster, "burn_only").await;
    let package = find::immutable(&fx).unwrap().0;
    let currency = find::address_owned_by(&fx, SUI_COIN_REGISTRY_ADDRESS.into()).unwrap();
    let coin = find::address_owned_by(&fx, sender).unwrap();
    let gas = fx.gas_object().0;

    let fx =
        coin_registry::finalize(&mut cluster, sender, &kp, package, "burn", currency, gas).await;

    cluster.create_checkpoint().await;
    let currency = find::shared(&fx).unwrap();
    let gas = fx.gas_object().0;

    let metadata = query_metadata(&cluster, &format!("{package}::burn::BURN")).await;
    assert_debug_snapshot!(metadata, @r###"
    CoinMetadataResponse {
        name: "Burn",
        decimals: 2,
        description: "A fake burn-only coin for test purposes",
        icon_url: Some(
            "https://example.com/fake.png",
        ),
        supply: Some(
            "1000000000",
        ),
        symbol: "BURN",
        supply_state: Some(
            "BURN_ONLY",
        ),
        regulated_state: Some(
            "UNREGULATED",
        ),
        allow_global_pause: Some(
            false,
        ),
    }
    "###);

    coin_registry::burn_from_currency(
        &mut cluster,
        sender,
        &kp,
        package,
        "burn",
        coin,
        100_000_000,
        currency,
        gas,
    )
    .await;

    cluster.create_checkpoint().await;
    assert_eq!(
        query_metadata(&cluster, &format!("{package}::burn::BURN")).await,
        CoinMetadataResponse {
            supply: Some("900000000".to_string()),
            ..metadata.clone()
        }
    );
}

#[tokio::test]
async fn test_unknown() {
    let mut cluster = FullCluster::new().await.unwrap();
    let (sender, kp, fx) = coin_registry::publish(&mut cluster, "unknown").await;
    let package = find::immutable(&fx).unwrap().0;
    let currency = find::address_owned_by(&fx, SUI_COIN_REGISTRY_ADDRESS.into()).unwrap();
    let coin = find::address_owned_by(&fx, sender).unwrap();
    let treasury_cap = find::shared(&fx).unwrap();
    let gas = fx.gas_object().0;

    let fx =
        coin_registry::finalize(&mut cluster, sender, &kp, package, "unknown", currency, gas).await;

    cluster.create_checkpoint().await;
    let gas = fx.gas_object().0;

    let metadata = query_metadata(&cluster, &format!("{package}::unknown::UNKNOWN")).await;
    assert_debug_snapshot!(metadata, @r###"
    CoinMetadataResponse {
        name: "Unknown",
        decimals: 2,
        description: "A fake unknown treasury coin for test purposes",
        icon_url: Some(
            "https://example.com/unknown.png",
        ),
        supply: Some(
            "1000000000",
        ),
        symbol: "UNKNOWN",
        supply_state: None,
        regulated_state: Some(
            "UNREGULATED",
        ),
        allow_global_pause: Some(
            false,
        ),
    }
    "###);

    let fx = coin_registry::burn_from_treasury(
        &mut cluster,
        sender,
        &kp,
        package,
        "unknown",
        coin,
        200_000_000,
        treasury_cap,
        gas,
    )
    .await;

    cluster.create_checkpoint().await;
    let gas = fx.gas_object().0;
    let coin = find::address_mutated(&fx).unwrap();

    // `supply` should reflect the burn operation.
    assert_eq!(
        query_metadata(&cluster, &format!("{package}::unknown::UNKNOWN")).await,
        CoinMetadataResponse {
            supply: Some("800000000".to_string()),
            ..metadata.clone()
        }
    );

    // Hide the treasury cap (move it to dynamic object field)
    let fx = coin_registry::hide_treasury_cap(
        &mut cluster,
        sender,
        &kp,
        package,
        "unknown",
        treasury_cap,
        gas,
    )
    .await;

    cluster.create_checkpoint().await;
    let gas = fx.gas_object().0;

    // `supply` should be `None` while the treasury cap is hidden.
    assert_eq!(
        query_metadata(&cluster, &format!("{package}::unknown::UNKNOWN")).await,
        CoinMetadataResponse {
            supply: None,
            ..metadata.clone()
        }
    );

    // Burn more while cap is hidden
    let fx = coin_registry::burn_from_treasury(
        &mut cluster,
        sender,
        &kp,
        package,
        "unknown",
        coin,
        100_000_000,
        treasury_cap,
        gas,
    )
    .await;

    cluster.create_checkpoint().await;
    let gas = fx.gas_object().0;

    // `supply` has been modified but it is still `None` while the treasury cap is hidden.
    assert_eq!(
        query_metadata(&cluster, &format!("{package}::unknown::UNKNOWN")).await,
        CoinMetadataResponse {
            supply: None,
            ..metadata.clone()
        }
    );

    // Remove the treasury cap from the dynamic field again.
    coin_registry::show_treasury_cap(
        &mut cluster,
        sender,
        &kp,
        package,
        "unknown",
        treasury_cap,
        gas,
    )
    .await;

    cluster.create_checkpoint().await;

    // `supply` is revealed and should reflect both burns (700M remaining)
    assert_eq!(
        query_metadata(&cluster, &format!("{package}::unknown::UNKNOWN")).await,
        CoinMetadataResponse {
            supply: Some("700000000".to_string()),
            ..metadata.clone()
        }
    );
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
        supply: Some(
            "1000000000",
        ),
        symbol: "LEGACY",
        supply_state: None,
        regulated_state: Some(
            "UNREGULATED",
        ),
        allow_global_pause: Some(
            false,
        ),
    }
    "###);

    // Migrate the legacy coin to the coin registry
    let fx = coin_registry::migrate(&mut cluster, sender, &kp, &outputs, gas).await;

    cluster.create_checkpoint().await;
    let outputs = query_owned_outputs(&cluster, sender).await;
    let currency = find::shared(&fx).unwrap(); // The migrated Currency<T> object
    let gas = fx.gas_object().0;

    // RPC output should be the same after the migration
    let migrated = query_metadata(&cluster, &outputs.coin_type.to_canonical_string(true)).await;
    assert_eq!(metadata, migrated);

    coin_registry::delete_migrated_legacy_metadata(
        &mut cluster,
        sender,
        &kp,
        &outputs,
        currency,
        gas,
    )
    .await;

    cluster.create_checkpoint().await;

    // RPC output should also be the same after deleting the legacy metadata
    let deleted = query_metadata(&cluster, &outputs.coin_type.to_canonical_string(true)).await;
    assert_eq!(metadata, deleted);
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
        supply: Some(
            "1000000000",
        ),
        symbol: "REGULATED",
        supply_state: Some(
            "FIXED",
        ),
        regulated_state: Some(
            "REGULATED",
        ),
        allow_global_pause: Some(
            true,
        ),
    }
    "###);
}

#[tokio::test]
async fn test_legacy_regulated_migrate_deny_cap() {
    let mut cluster = FullCluster::new().await.unwrap();
    let (sender, kp, fx) = coin_registry::publish(&mut cluster, "legacy_regulated").await;

    cluster.create_checkpoint().await;
    let outputs = query_owned_outputs(&cluster, sender).await;
    let gas = fx.gas_object().0;

    let metadata = query_metadata(&cluster, &outputs.coin_type.to_canonical_string(true)).await;
    assert_debug_snapshot!(metadata, @r###"
    CoinMetadataResponse {
        name: "LegacyRegulated",
        decimals: 2,
        description: "A fake legacy regulated coin for test purposes",
        icon_url: Some(
            "https://example.com/regulated.png",
        ),
        supply: Some(
            "1000000000",
        ),
        symbol: "REGULATED",
        supply_state: None,
        regulated_state: Some(
            "REGULATED",
        ),
        allow_global_pause: None,
    }
    "###);

    // Migrate the legacy coin to the coin registry
    let fx = coin_registry::migrate(&mut cluster, sender, &kp, &outputs, gas).await;

    cluster.create_checkpoint().await;
    let outputs = query_owned_outputs(&cluster, sender).await;
    let currency = find::shared(&fx).unwrap(); // The migrated Currency<T> object
    let gas = fx.gas_object().0;

    // Query the coin metadata again after migration - should produce the same results
    let migrated = query_metadata(&cluster, &outputs.coin_type.to_canonical_string(true)).await;
    assert_eq!(metadata, migrated);

    let fx =
        coin_registry::migrate_deny_cap(&mut cluster, sender, &kp, &outputs, currency, gas).await;

    cluster.create_checkpoint().await;
    let outputs = query_owned_outputs(&cluster, sender).await;
    let gas = fx.gas_object().0;

    // After migrating the deny cap, `allow_global_pause` is `false` but the rest is the same.
    let migrated = query_metadata(&cluster, &outputs.coin_type.to_canonical_string(true)).await;
    assert_eq!(
        migrated,
        CoinMetadataResponse {
            allow_global_pause: Some(false),
            ..metadata.clone()
        }
    );

    coin_registry::delete_migrated_legacy_metadata(
        &mut cluster,
        sender,
        &kp,
        &outputs,
        currency,
        gas,
    )
    .await;

    cluster.create_checkpoint().await;
    let outputs = query_owned_outputs(&cluster, sender).await;

    // RPC response should be unchanged after deleting the legacy metadata.
    let deleted = query_metadata(&cluster, &outputs.coin_type.to_canonical_string(true)).await;
    assert_eq!(deleted, migrated);
}

#[tokio::test]
async fn test_legacy_regulated_migrate_regulated_metadata() {
    let mut cluster = FullCluster::new().await.unwrap();
    let (sender, kp, fx) = coin_registry::publish(&mut cluster, "legacy_regulated").await;

    cluster.create_checkpoint().await;
    let outputs = query_owned_outputs(&cluster, sender).await;
    let regulated_metadata = fx
        .created()
        .into_iter()
        .find_map(|(oref, o)| {
            matches!(o, Owner::Immutable if oref.0 != outputs.coin_type.address.into())
                .then_some(oref)
        })
        .unwrap();
    let gas = fx.gas_object().0;

    let metadata = query_metadata(&cluster, &outputs.coin_type.to_canonical_string(true)).await;
    assert_debug_snapshot!(metadata, @r###"
    CoinMetadataResponse {
        name: "LegacyRegulated",
        decimals: 2,
        description: "A fake legacy regulated coin for test purposes",
        icon_url: Some(
            "https://example.com/regulated.png",
        ),
        supply: Some(
            "1000000000",
        ),
        symbol: "REGULATED",
        supply_state: None,
        regulated_state: Some(
            "REGULATED",
        ),
        allow_global_pause: None,
    }
    "###);

    // Migrate the legacy coin to the coin registry
    let fx = coin_registry::migrate(&mut cluster, sender, &kp, &outputs, gas).await;

    cluster.create_checkpoint().await;
    let outputs = query_owned_outputs(&cluster, sender).await;
    let currency = find::shared(&fx).unwrap(); // The migrated Currency<T> object
    let gas = fx.gas_object().0;

    // Query the coin metadata again after migration - should produce the same results
    let migrated = query_metadata(&cluster, &outputs.coin_type.to_canonical_string(true)).await;
    assert_eq!(metadata, migrated);

    let fx = coin_registry::migrate_regulated_metadata(
        &mut cluster,
        sender,
        &kp,
        &outputs,
        currency,
        regulated_metadata,
        gas,
    )
    .await;

    cluster.create_checkpoint().await;
    let outputs = query_owned_outputs(&cluster, sender).await;
    let gas = fx.gas_object().0;

    // After migrating the metadata, the output is the same (migration from metadata doesn't port
    // the `allow_global_pause` field).
    let migrated = query_metadata(&cluster, &outputs.coin_type.to_canonical_string(true)).await;
    assert_eq!(migrated, metadata);

    coin_registry::delete_migrated_legacy_metadata(
        &mut cluster,
        sender,
        &kp,
        &outputs,
        currency,
        gas,
    )
    .await;
    cluster.create_checkpoint().await;
    let outputs = query_owned_outputs(&cluster, sender).await;

    // RPC response should also be unchanged after deleting the legacy metadata.
    let deleted = query_metadata(&cluster, &outputs.coin_type.to_canonical_string(true)).await;
    assert_eq!(deleted, metadata);
}

/// Run a GraphQL query to fetch the coin metadata for `coin_type` from `cluster`.
async fn query_metadata(cluster: &FullCluster, coin_type: &str) -> CoinMetadataResponse {
    let client = reqwest::Client::new();
    let url = cluster.graphql_url();

    let response: serde_json::Value = client
        .post(url.as_str())
        .json(&json!({
            "query": METADATA_QUERY,
            "variables": { "coinType": coin_type }
        }))
        .send()
        .await
        .unwrap()
        .json()
        .await
        .unwrap();

    let metadata = response.pointer("/data/coinMetadata").unwrap();
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

/// Query all the owned objects of `owner` using the GraphQL API on `cluster`.
async fn query_objects(cluster: &FullCluster, owner: SuiAddress) -> Vec<(StructTag, ObjectRef)> {
    #[derive(Deserialize)]
    #[serde(rename_all = "camelCase")]
    struct PageInfo {
        end_cursor: Option<String>,
        has_next_page: bool,
    }

    #[derive(Deserialize)]
    #[serde(rename_all = "camelCase")]
    struct Node {
        address: String,
        version: u64,
        digest: String,
        contents: serde_json::Value,
    }

    #[derive(Deserialize)]
    #[serde(rename_all = "camelCase")]
    struct Connection {
        page_info: PageInfo,
        nodes: Vec<Node>,
    }

    let client = reqwest::Client::new();
    let url = cluster.graphql_url();

    let mut objects = vec![];
    let mut after: Option<String> = None;

    loop {
        let response: serde_json::Value = client
            .post(url.as_str())
            .json(&json!({
                "query": OBJECTS_QUERY,
                "variables": {
                    "owner":  owner.to_string(),
                    "after": after,
                },
            }))
            .send()
            .await
            .unwrap()
            .json()
            .await
            .unwrap();

        let value: &serde_json::Value = response.pointer("/data/address/objects").unwrap();
        let Connection { page_info, nodes } = serde_json::from_value(value.clone()).unwrap();

        for node in nodes {
            let address: ObjectID = node.address.parse().unwrap();
            let version: SequenceNumber = node.version.into();
            let digest: ObjectDigest = node.digest.parse().unwrap();
            let type_: StructTag = node
                .contents
                .pointer("/type/repr")
                .unwrap()
                .as_str()
                .unwrap()
                .parse()
                .unwrap();

            objects.push((type_, (address, version, digest)));
        }

        if !page_info.has_next_page {
            break;
        } else {
            after = page_info.end_cursor;
        }
    }

    objects
}
