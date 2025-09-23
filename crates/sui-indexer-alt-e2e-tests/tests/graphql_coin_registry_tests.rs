// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

use std::path::PathBuf;

use fastcrypto::ed25519::Ed25519KeyPair;
use insta::assert_debug_snapshot;
use move_core_types::{ident_str, language_storage::StructTag};
use serde::Deserialize;
use serde_json::json;
use sui_indexer_alt_e2e_tests::{
    find_address_mutated, find_address_owned_by, find_immutable, find_shared, FullCluster,
};
use sui_move_build::BuildConfig;
use sui_types::{
    base_types::{ObjectID, ObjectRef, SequenceNumber, SuiAddress},
    coin::{CoinMetadata, TreasuryCap},
    deny_list_v2::DenyCapV2,
    digests::ObjectDigest,
    effects::{TransactionEffects, TransactionEffectsAPI},
    object::Owner,
    programmable_transaction_builder::ProgrammableTransactionBuilder,
    transaction::{CallArg, Command, ObjectArg, Transaction, TransactionData},
    Identifier, TypeTag, SUI_COIN_REGISTRY_ADDRESS, SUI_COIN_REGISTRY_OBJECT_ID,
    SUI_FRAMEWORK_PACKAGE_ID,
};

/// 5 SUI gas budget
const DEFAULT_GAS_BUDGET: u64 = 5_000_000_000;

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

/// Output from querying owned objects related to creating a new legacy currency.
struct LegacyCoinOutputs {
    coin_type: StructTag,
    treasury: ObjectRef,
    metadata: Option<ObjectRef>,
    deny: Option<ObjectRef>,
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
    let (a, kp, fx) = publish(&mut cluster, "fixed_supply").await;
    let package = find_immutable(&fx).unwrap().0;
    let currency = find_address_owned_by(&fx, SUI_COIN_REGISTRY_ADDRESS.into()).unwrap();
    let gas = fx.gas_object().0;

    finalize(&mut cluster, a, &kp, package, "fixed", currency, gas).await;
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
    let (sender, kp, fx) = publish(&mut cluster, "dynamic").await;
    let package = find_immutable(&fx).unwrap().0;
    let gas = fx.gas_object().0;

    // Create a dynamic currency
    let mut builder = ProgrammableTransactionBuilder::new();
    builder
        .move_call(
            package,
            ident_str!("dynamic").to_owned(),
            ident_str!("new_currency").to_owned(),
            vec![],
            vec![CallArg::Object(ObjectArg::SharedObject {
                id: SUI_COIN_REGISTRY_OBJECT_ID,
                initial_shared_version: 1.into(),
                mutable: true,
            })],
        )
        .unwrap();

    let data = TransactionData::new_programmable(
        sender,
        vec![gas],
        builder.finish(),
        DEFAULT_GAS_BUDGET,
        cluster.reference_gas_price(),
    );

    let (_fx, error) = cluster
        .execute_transaction(Transaction::from_data_and_signer(data, vec![&kp]))
        .expect("new_currency failed");

    assert!(error.is_none(), "new_currency failed: {error:?}");
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
    let (sender, kp, fx) = publish(&mut cluster, "burn_only").await;
    let package = find_immutable(&fx).unwrap().0;
    let currency = find_address_owned_by(&fx, SUI_COIN_REGISTRY_ADDRESS.into()).unwrap();
    let coin = find_address_owned_by(&fx, sender).unwrap();
    let gas = fx.gas_object().0;

    let fx = finalize(&mut cluster, sender, &kp, package, "burn", currency, gas).await;

    cluster.create_checkpoint().await;
    let currency = find_shared(&fx).unwrap();
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

    burn_from_currency(
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
    let (sender, kp, fx) = publish(&mut cluster, "unknown").await;
    let package = find_immutable(&fx).unwrap().0;
    let currency = find_address_owned_by(&fx, SUI_COIN_REGISTRY_ADDRESS.into()).unwrap();
    let coin = find_address_owned_by(&fx, sender).unwrap();
    let treasury_cap = find_shared(&fx).unwrap();
    let gas = fx.gas_object().0;

    let fx = finalize(&mut cluster, sender, &kp, package, "unknown", currency, gas).await;

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

    let fx = burn_from_treasury(
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
    let coin = find_address_mutated(&fx).unwrap();

    // `supply` should reflect the burn operation.
    assert_eq!(
        query_metadata(&cluster, &format!("{package}::unknown::UNKNOWN")).await,
        CoinMetadataResponse {
            supply: Some("800000000".to_string()),
            ..metadata.clone()
        }
    );

    // Hide the treasury cap (move it to dynamic object field)
    let fx = hide_treasury_cap(
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
    let fx = burn_from_treasury(
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
    show_treasury_cap(
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
    let (sender, kp, fx) = publish(&mut cluster, "legacy").await;

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
    let fx = migrate(&mut cluster, sender, &kp, &outputs, gas).await;

    cluster.create_checkpoint().await;
    let outputs = query_owned_outputs(&cluster, sender).await;
    let currency = find_shared(&fx).unwrap(); // The migrated Currency<T> object
    let gas = fx.gas_object().0;

    // RPC output should be the same after the migration
    let migrated = query_metadata(&cluster, &outputs.coin_type.to_canonical_string(true)).await;
    assert_eq!(metadata, migrated);

    delete_migrated_legacy_metadata(&mut cluster, sender, &kp, &outputs, currency, gas).await;
    cluster.create_checkpoint().await;

    // RPC output should also be the same after deleting the legacy metadata
    let deleted = query_metadata(&cluster, &outputs.coin_type.to_canonical_string(true)).await;
    assert_eq!(metadata, deleted);
}

#[tokio::test]
async fn test_regulated() {
    let mut cluster = FullCluster::new().await.unwrap();
    let (a, kp, fx) = publish(&mut cluster, "regulated").await;
    let package = find_immutable(&fx).unwrap().0;
    let currency = find_address_owned_by(&fx, SUI_COIN_REGISTRY_ADDRESS.into()).unwrap();
    let gas = fx.gas_object().0;

    finalize(&mut cluster, a, &kp, package, "regulated", currency, gas).await;
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
    let (sender, kp, fx) = publish(&mut cluster, "legacy_regulated").await;

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
    let fx = migrate(&mut cluster, sender, &kp, &outputs, gas).await;

    cluster.create_checkpoint().await;
    let outputs = query_owned_outputs(&cluster, sender).await;
    let currency = find_shared(&fx).unwrap(); // The migrated Currency<T> object
    let gas = fx.gas_object().0;

    // Query the coin metadata again after migration - should produce the same results
    let migrated = query_metadata(&cluster, &outputs.coin_type.to_canonical_string(true)).await;
    assert_eq!(metadata, migrated);

    let fx = migrate_deny_cap(&mut cluster, sender, &kp, &outputs, currency, gas).await;

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

    delete_migrated_legacy_metadata(&mut cluster, sender, &kp, &outputs, currency, gas).await;
    cluster.create_checkpoint().await;
    let outputs = query_owned_outputs(&cluster, sender).await;

    // RPC response should be unchanged after deleting the legacy metadata.
    let deleted = query_metadata(&cluster, &outputs.coin_type.to_canonical_string(true)).await;
    assert_eq!(deleted, migrated);
}

#[tokio::test]
async fn test_legacy_regulated_migrate_regulated_metadata() {
    let mut cluster = FullCluster::new().await.unwrap();
    let (sender, kp, fx) = publish(&mut cluster, "legacy_regulated").await;

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
    let fx = migrate(&mut cluster, sender, &kp, &outputs, gas).await;

    cluster.create_checkpoint().await;
    let outputs = query_owned_outputs(&cluster, sender).await;
    let currency = find_shared(&fx).unwrap(); // The migrated Currency<T> object
    let gas = fx.gas_object().0;

    // Query the coin metadata again after migration - should produce the same results
    let migrated = query_metadata(&cluster, &outputs.coin_type.to_canonical_string(true)).await;
    assert_eq!(metadata, migrated);

    let fx = migrate_regulated_metadata(
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

    delete_migrated_legacy_metadata(&mut cluster, sender, &kp, &outputs, currency, gas).await;
    cluster.create_checkpoint().await;
    let outputs = query_owned_outputs(&cluster, sender).await;

    // RPC response should also be unchanged after deleting the legacy metadata.
    let deleted = query_metadata(&cluster, &outputs.coin_type.to_canonical_string(true)).await;
    assert_eq!(deleted, metadata);
}

/// Publish `packages/coin_registry/<package>` to `cluster` with a fresh, funded account.
///
/// Returns the address and keypair of the publishing address, and the effects of the publish
/// transaction.
async fn publish(
    cluster: &mut FullCluster,
    package: &str,
) -> (SuiAddress, Ed25519KeyPair, TransactionEffects) {
    // Compile the package
    let mut path = PathBuf::from(env!("CARGO_MANIFEST_DIR"));
    path.extend(["packages", "coin_registry", package]);

    let pkg = BuildConfig::new_for_testing()
        .build(&path)
        .expect("Failed to compile package");

    // Create an address and fund it to run the following transactions.
    let (sender, kp, gas) = cluster.funded_account(1000 * DEFAULT_GAS_BUDGET).unwrap();

    // Build the publish transaction
    let mut builder = ProgrammableTransactionBuilder::new();
    let with_unpublished_deps = false;
    builder.publish_immutable(
        pkg.get_package_bytes(with_unpublished_deps),
        pkg.get_dependency_storage_package_ids(),
    );

    let data = TransactionData::new_programmable(
        sender,
        vec![gas],
        builder.finish(),
        DEFAULT_GAS_BUDGET,
        cluster.reference_gas_price(),
    );

    // Sign and execute the transaction
    let (fx, error) = cluster
        .execute_transaction(Transaction::from_data_and_signer(data, vec![&kp]))
        .expect("Publish failed");

    assert!(error.is_none(), "Publish failed: {error:?}");
    (sender, kp, fx)
}

/// Finalize a `Currency<T>` that was sent to the `CoinRegistry` during a package publish.
///
/// - `sender` and `kp` describe the account that will sign and pay for the transaction.
/// - `package` and `module` identify the coin type `T` as `<package>::<module>::<MODULE>`.
/// - `currency` is the `Currency<T>` object that was created during the package publish, and sent
///   to the `CoinRegistry`.
/// - `gas` is the gas object to use for the transaction, which must be owned by `sender`.
///
/// Returns the effects of running the finalize transaction.
async fn finalize(
    cluster: &mut FullCluster,
    sender: SuiAddress,
    kp: &Ed25519KeyPair,
    package: ObjectID,
    module: &str,
    currency: ObjectRef,
    gas: ObjectRef,
) -> TransactionEffects {
    let mut builder = ProgrammableTransactionBuilder::new();
    builder
        .move_call(
            SUI_FRAMEWORK_PACKAGE_ID,
            ident_str!("coin_registry").to_owned(),
            ident_str!("finalize_registration").to_owned(),
            vec![TypeTag::Struct(Box::new(StructTag {
                address: package.into(),
                module: Identifier::new(module).unwrap(),
                name: Identifier::new(module.to_owned().to_ascii_uppercase()).unwrap(),
                type_params: vec![],
            }))],
            vec![
                CallArg::Object(ObjectArg::SharedObject {
                    id: SUI_COIN_REGISTRY_OBJECT_ID,
                    initial_shared_version: 1.into(),
                    mutable: true,
                }),
                CallArg::Object(ObjectArg::Receiving(currency)),
            ],
        )
        .unwrap();

    let data = TransactionData::new_programmable(
        sender,
        vec![gas],
        builder.finish(),
        DEFAULT_GAS_BUDGET,
        cluster.reference_gas_price(),
    );

    let (fx, error) = cluster
        .execute_transaction(Transaction::from_data_and_signer(data, vec![kp]))
        .expect("Publish failed");

    assert!(error.is_none(), "Finalize failed: {error:?}");
    fx
}

/// Burn `amount` from a coin using the Currency object.
///
/// - `sender` and `kp` describe the account that will sign and pay for the transaction.
/// - `package` and `module` identify the coin type `T` as `<package>::<module>::<MODULE>`.
/// - `currency` is the `Currency<T>` object that was created during finalization (a shared object
///   with a derived address).
/// - `coin` is the coin object to split and burn from.
/// - `gas` is the gas object to use for the transaction, which must be owned by `sender`.
/// - `amount` is the amount to burn.
///
/// Returns the effects of running the burn transaction.
async fn burn_from_currency(
    cluster: &mut FullCluster,
    sender: SuiAddress,
    kp: &Ed25519KeyPair,
    package: ObjectID,
    module: &str,
    coin: ObjectRef,
    amount: u64,
    currency: ObjectRef,
    gas: ObjectRef,
) -> TransactionEffects {
    let mut builder = ProgrammableTransactionBuilder::new();

    let coin = builder.obj(ObjectArg::ImmOrOwnedObject(coin)).unwrap();
    let amount = builder.pure(amount).unwrap();
    let currency = builder
        .obj(ObjectArg::SharedObject {
            id: currency.0,
            initial_shared_version: currency.1,
            mutable: true,
        })
        .unwrap();

    let split = builder.command(Command::SplitCoins(coin, vec![amount]));
    builder.programmable_move_call(
        SUI_FRAMEWORK_PACKAGE_ID,
        ident_str!("coin_registry").to_owned(),
        ident_str!("burn").to_owned(),
        vec![TypeTag::Struct(Box::new(StructTag {
            address: package.into(),
            module: Identifier::new(module).unwrap(),
            name: Identifier::new(module.to_owned().to_ascii_uppercase()).unwrap(),
            type_params: vec![],
        }))],
        vec![currency, split],
    );

    let data = TransactionData::new_programmable(
        sender,
        vec![gas],
        builder.finish(),
        DEFAULT_GAS_BUDGET,
        cluster.reference_gas_price(),
    );

    let (fx, error) = cluster
        .execute_transaction(Transaction::from_data_and_signer(data, vec![kp]))
        .expect("Burn failed");

    assert!(error.is_none(), "Burn failed: {error:?}");
    fx
}

/// Burn `amount` from a coin using a `TreasuryCap` wrapped in a shared `Treasury` object.
///
/// - `sender` and `kp` describe the account that will sign and pay for the transaction.
/// - `package` and `module` identify the coin type `T` as `<package>::<module>::<MODULE>`.
/// - `coin` is the coin object to burn.
/// - `amount` is the amount to burn.
/// - `treasury_cap` is the shared TreasuryCap object.
/// - `gas` is the gas object to use for the transaction, which must be owned by `sender`.
///
/// Returns the effects of running the burn transaction.
async fn burn_from_treasury(
    cluster: &mut FullCluster,
    sender: SuiAddress,
    kp: &Ed25519KeyPair,
    package: ObjectID,
    module: &str,
    coin: ObjectRef,
    amount: u64,
    treasury: ObjectRef,
    gas: ObjectRef,
) -> TransactionEffects {
    let mut builder = ProgrammableTransactionBuilder::new();

    let coin = builder.obj(ObjectArg::ImmOrOwnedObject(coin)).unwrap();
    let amount = builder.pure(amount).unwrap();
    let treasury = builder
        .obj(ObjectArg::SharedObject {
            id: treasury.0,
            initial_shared_version: treasury.1,
            mutable: true,
        })
        .unwrap();

    // Split the coin to get the amount we want to burn
    let split = builder.command(Command::SplitCoins(coin, vec![amount]));

    // Burn directly using coin::burn
    builder.programmable_move_call(
        package,
        Identifier::new(module).unwrap(),
        ident_str!("burn").to_owned(),
        vec![],
        vec![treasury, split],
    );

    let data = TransactionData::new_programmable(
        sender,
        vec![gas],
        builder.finish(),
        DEFAULT_GAS_BUDGET,
        cluster.reference_gas_price(),
    );

    let (fx, error) = cluster
        .execute_transaction(Transaction::from_data_and_signer(data, vec![kp]))
        .expect("Treasury burn failed");

    assert!(error.is_none(), "Treasury burn failed: {error:?}");
    fx
}

/// Hide the treasury cap in the Treasury object (using dynamic object field).
///
/// - `sender` and `kp` describe the account that will sign and pay for the transaction.
/// - `package` and `module` identify the package containing the hide function.
/// - `treasury` is the shared Treasury object.
/// - `gas` is the gas object to use for the transaction, which must be owned by `sender`.
///
/// Returns the effects of running the hide transaction.
async fn hide_treasury_cap(
    cluster: &mut FullCluster,
    sender: SuiAddress,
    kp: &Ed25519KeyPair,
    package: ObjectID,
    module: &str,
    treasury: ObjectRef,
    gas: ObjectRef,
) -> TransactionEffects {
    let mut builder = ProgrammableTransactionBuilder::new();

    let treasury = builder
        .obj(ObjectArg::SharedObject {
            id: treasury.0,
            initial_shared_version: treasury.1,
            mutable: true,
        })
        .unwrap();

    builder.programmable_move_call(
        package,
        Identifier::new(module).unwrap(),
        ident_str!("hide").to_owned(),
        vec![],
        vec![treasury],
    );

    let data = TransactionData::new_programmable(
        sender,
        vec![gas],
        builder.finish(),
        DEFAULT_GAS_BUDGET,
        cluster.reference_gas_price(),
    );

    let (fx, error) = cluster
        .execute_transaction(Transaction::from_data_and_signer(data, vec![kp]))
        .expect("Hide treasury cap failed");

    assert!(error.is_none(), "Hide treasury cap failed: {error:?}");
    fx
}

/// Show the treasury cap in the Treasury object (move from dynamic object field to Option field).
///
/// - `sender` and `kp` describe the account that will sign and pay for the transaction.
/// - `package` and `module` identify the package containing the show function.
/// - `treasury` is the shared Treasury object.
/// - `gas` is the gas object to use for the transaction, which must be owned by `sender`.
///
/// Returns the effects of running the show transaction.
async fn show_treasury_cap(
    cluster: &mut FullCluster,
    sender: SuiAddress,
    kp: &Ed25519KeyPair,
    package: ObjectID,
    module: &str,
    treasury: ObjectRef,
    gas: ObjectRef,
) -> TransactionEffects {
    let mut builder = ProgrammableTransactionBuilder::new();

    let treasury = builder
        .obj(ObjectArg::SharedObject {
            id: treasury.0,
            initial_shared_version: treasury.1,
            mutable: true,
        })
        .unwrap();

    builder.programmable_move_call(
        package,
        Identifier::new(module).unwrap(),
        ident_str!("show").to_owned(),
        vec![],
        vec![treasury],
    );

    let data = TransactionData::new_programmable(
        sender,
        vec![gas],
        builder.finish(),
        DEFAULT_GAS_BUDGET,
        cluster.reference_gas_price(),
    );

    let (fx, error) = cluster
        .execute_transaction(Transaction::from_data_and_signer(data, vec![kp]))
        .expect("Show treasury cap failed");

    assert!(error.is_none(), "Show treasury cap failed: {error:?}");
    fx
}

/// Migrate a legacy CoinMetadata object to the coin registry.
///
/// - `sender` and `kp` describe the account that will sign and pay for the transaction.
/// - `outputs` describes owned outputs of currency creation.
/// - `gas` is the gas object to use for the transaction, which must be owned by `sender`.
///
/// Returns the effects of running the migration transaction.
async fn migrate(
    cluster: &mut FullCluster,
    sender: SuiAddress,
    kp: &Ed25519KeyPair,
    outputs: &LegacyCoinOutputs,
    gas: ObjectRef,
) -> TransactionEffects {
    let mut builder = ProgrammableTransactionBuilder::new();

    let registry = builder
        .obj(ObjectArg::SharedObject {
            id: SUI_COIN_REGISTRY_OBJECT_ID,
            initial_shared_version: 1.into(),
            mutable: true,
        })
        .unwrap();

    let metadata = builder
        .obj(ObjectArg::ImmOrOwnedObject(outputs.metadata.unwrap()))
        .unwrap();

    builder.programmable_move_call(
        SUI_FRAMEWORK_PACKAGE_ID,
        ident_str!("coin_registry").to_owned(),
        ident_str!("migrate_legacy_metadata").to_owned(),
        vec![TypeTag::Struct(Box::new(outputs.coin_type.clone()))],
        vec![registry, metadata],
    );

    let data = TransactionData::new_programmable(
        sender,
        vec![gas],
        builder.finish(),
        DEFAULT_GAS_BUDGET,
        cluster.reference_gas_price(),
    );

    let (fx, error) = cluster
        .execute_transaction(Transaction::from_data_and_signer(data, vec![kp]))
        .expect("Migration failed");

    assert!(error.is_none(), "Migration failed: {error:?}");
    fx
}

/// Migrate the regulated state of the currency based on its `DenyCapV2`.
///
/// - `sender` and `kp` describe the account that will sign and pay for the transaction.
/// - `outputs` describes owned outputs of currency creation.
/// - `currency` is the `Currency<T>` object.
/// - `gas` is the gas object to use for the transaction, which must be owned by `sender`.
///
/// Returns the effects of running the migration transaction.
async fn migrate_deny_cap(
    cluster: &mut FullCluster,
    sender: SuiAddress,
    kp: &Ed25519KeyPair,
    outputs: &LegacyCoinOutputs,
    currency: ObjectRef,
    gas: ObjectRef,
) -> TransactionEffects {
    let mut builder = ProgrammableTransactionBuilder::new();

    let currency = builder
        .obj(ObjectArg::SharedObject {
            id: currency.0,
            initial_shared_version: currency.1,
            mutable: true,
        })
        .unwrap();

    let deny_cap = builder
        .obj(ObjectArg::ImmOrOwnedObject(outputs.deny.unwrap()))
        .unwrap();

    builder.programmable_move_call(
        SUI_FRAMEWORK_PACKAGE_ID,
        ident_str!("coin_registry").to_owned(),
        ident_str!("migrate_regulated_state_by_cap").to_owned(),
        vec![TypeTag::Struct(Box::new(outputs.coin_type.clone()))],
        vec![currency, deny_cap],
    );

    let data = TransactionData::new_programmable(
        sender,
        vec![gas],
        builder.finish(),
        DEFAULT_GAS_BUDGET,
        cluster.reference_gas_price(),
    );

    let (fx, error) = cluster
        .execute_transaction(Transaction::from_data_and_signer(data, vec![kp]))
        .expect("Deny cap migration failed");

    assert!(error.is_none(), "Deny cap migration failed: {error:?}");
    fx
}

/// Migrate the regulated state of the currency based on its `RegulatedCoinMetadata`.
///
/// - `sender` and `kp` describe the account that will sign and pay for the transaction.
/// - `outputs` describes owned outputs of currency creation.
/// - `currency` is the `Currency<T>` object.
/// - `regulated_metadata` is the `RegulatedCoinMetadata` object.
/// - `gas` is the gas object to use for the transaction, which must be owned by `sender`.
///
/// Returns the effects of running the migration transaction.
async fn migrate_regulated_metadata(
    cluster: &mut FullCluster,
    sender: SuiAddress,
    kp: &Ed25519KeyPair,
    outputs: &LegacyCoinOutputs,
    currency: ObjectRef,
    regulated_metadata: ObjectRef,
    gas: ObjectRef,
) -> TransactionEffects {
    let mut builder = ProgrammableTransactionBuilder::new();

    let currency = builder
        .obj(ObjectArg::SharedObject {
            id: currency.0,
            initial_shared_version: currency.1,
            mutable: true,
        })
        .unwrap();

    let regulated_metadata = builder
        .obj(ObjectArg::ImmOrOwnedObject(regulated_metadata))
        .unwrap();

    builder.programmable_move_call(
        SUI_FRAMEWORK_PACKAGE_ID,
        ident_str!("coin_registry").to_owned(),
        ident_str!("migrate_regulated_state_by_metadata").to_owned(),
        vec![TypeTag::Struct(Box::new(outputs.coin_type.clone()))],
        vec![currency, regulated_metadata],
    );

    let data = TransactionData::new_programmable(
        sender,
        vec![gas],
        builder.finish(),
        DEFAULT_GAS_BUDGET,
        cluster.reference_gas_price(),
    );

    let (fx, error) = cluster
        .execute_transaction(Transaction::from_data_and_signer(data, vec![kp]))
        .expect("Deny cap migration failed");

    assert!(error.is_none(), "Metadata migration failed: {error:?}");
    fx
}

/// Delete the migrated legacy metadata.
///
/// - `sender` and `kp` describe the account that will sign and pay for the transaction.
/// - `outputs` describes owned outputs of currency creation.
/// - `currency` is the `Currency<T>` object.
/// - `gas` is the gas object to use for the transaction, which must be owned by `sender`.
///
/// Returns the effects of running the delete transaction.
async fn delete_migrated_legacy_metadata(
    cluster: &mut FullCluster,
    sender: SuiAddress,
    kp: &Ed25519KeyPair,
    outputs: &LegacyCoinOutputs,
    currency: ObjectRef,
    gas: ObjectRef,
) -> TransactionEffects {
    let mut builder = ProgrammableTransactionBuilder::new();

    let currency = builder
        .obj(ObjectArg::SharedObject {
            id: currency.0,
            initial_shared_version: currency.1,
            mutable: true,
        })
        .unwrap();

    let treasury_cap = builder
        .obj(ObjectArg::ImmOrOwnedObject(outputs.treasury))
        .unwrap();

    let legacy_metadata = builder
        .obj(ObjectArg::ImmOrOwnedObject(outputs.metadata.unwrap()))
        .unwrap();

    // Claim the metadata cap
    let metadata_cap = builder.programmable_move_call(
        SUI_FRAMEWORK_PACKAGE_ID,
        ident_str!("coin_registry").to_owned(),
        ident_str!("claim_metadata_cap").to_owned(),
        vec![TypeTag::Struct(Box::new(outputs.coin_type.clone()))],
        vec![currency, treasury_cap],
    );

    // Delete the metadata cap
    builder.programmable_move_call(
        SUI_FRAMEWORK_PACKAGE_ID,
        ident_str!("coin_registry").to_owned(),
        ident_str!("delete_metadata_cap").to_owned(),
        vec![TypeTag::Struct(Box::new(outputs.coin_type.clone()))],
        vec![currency, metadata_cap],
    );

    // Delete the migrated legacy metadata
    builder.programmable_move_call(
        SUI_FRAMEWORK_PACKAGE_ID,
        ident_str!("coin_registry").to_owned(),
        ident_str!("delete_migrated_legacy_metadata").to_owned(),
        vec![TypeTag::Struct(Box::new(outputs.coin_type.clone()))],
        vec![currency, legacy_metadata],
    );

    let data = TransactionData::new_programmable(
        sender,
        vec![gas],
        builder.finish(),
        DEFAULT_GAS_BUDGET,
        cluster.reference_gas_price(),
    );

    let (fx, error) = cluster
        .execute_transaction(Transaction::from_data_and_signer(data, vec![kp]))
        .expect("Delete migrated legacy metadata failed");

    assert!(error.is_none(), "Deletion failed: {error:?}");
    fx
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
