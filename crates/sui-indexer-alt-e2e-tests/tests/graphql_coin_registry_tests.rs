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
    base_types::{ObjectID, ObjectRef, SuiAddress},
    effects::{TransactionEffects, TransactionEffectsAPI},
    programmable_transaction_builder::ProgrammableTransactionBuilder,
    transaction::{CallArg, Command, ObjectArg, Transaction, TransactionData},
    Identifier, TypeTag, SUI_COIN_REGISTRY_ADDRESS, SUI_COIN_REGISTRY_OBJECT_ID,
    SUI_FRAMEWORK_PACKAGE_ID,
};

/// 5 SUI gas budget
const DEFAULT_GAS_BUDGET: u64 = 5_000_000_000;

const QUERY: &str = r#"
query GetCoinMetadata($coinType: String!) {
    coinMetadata(coinType: $coinType) {
        name
        decimals
        description
        iconUrl
        supply
        symbol
    }
}
"#;

#[derive(Deserialize, Eq, PartialEq, Debug)]
#[serde(rename_all = "camelCase")]
struct CoinMetadata {
    name: String,
    decimals: u8,
    description: String,
    icon_url: Option<String>,
    supply: Option<String>,
    symbol: String,
}

#[tokio::test]
async fn test_sui() {
    let mut cluster = FullCluster::new().await.unwrap();

    // SUI coin is available from genesis, no need to publish

    // Generate and index checkpoint
    cluster.create_checkpoint().await;

    // Query the coin metadata for SUI
    let metadata = query_metadata(&cluster, "0x2::sui::SUI").await;
    assert_debug_snapshot!(metadata, @r###"
    CoinMetadata {
        name: "Sui",
        decimals: 9,
        description: "",
        icon_url: None,
        supply: Some(
            "10000000000000000000",
        ),
        symbol: "SUI",
    }
    "###);
}

#[tokio::test]
async fn test_fixed_supply() {
    let mut cluster = FullCluster::new().await.unwrap();

    // Publish the fixed supply coin
    let (a, kp, fx) = publish(&mut cluster, "fixed_supply").await;
    let package = find_immutable(&fx).unwrap().0;
    let currency = find_address_owned_by(&fx, SUI_COIN_REGISTRY_ADDRESS.into()).unwrap();
    let gas = fx.gas_object().0;

    // Finalize the registration
    finalize(&mut cluster, a, &kp, package, "fixed", currency, gas).await;

    // Generate and index checkpoint
    cluster.create_checkpoint().await;

    // Query the coin metadata
    let metadata = query_metadata(&cluster, &format!("{package}::fixed::FIXED")).await;
    assert_debug_snapshot!(metadata, @r###"
    CoinMetadata {
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
    }
    "###);
}

#[tokio::test]
async fn test_dynamic() {
    let mut cluster = FullCluster::new().await.unwrap();

    // Publish the dynamic coin package
    let (sender, kp, fx) = publish(&mut cluster, "dynamic").await;
    let package = find_immutable(&fx).unwrap().0;
    let gas = fx.gas_object().0;

    // Call new_currency to dynamically create the currency
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

    // Generate and index checkpoint
    cluster.create_checkpoint().await;

    // Query the coin metadata for the dynamically created coin
    let metadata = query_metadata(&cluster, &format!("{package}::dynamic::Dynamic")).await;
    assert_debug_snapshot!(metadata, @r###"
    CoinMetadata {
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
    }
    "###);
}

#[tokio::test]
async fn test_burn_only() {
    let mut cluster = FullCluster::new().await.unwrap();

    // Publish the burn only coin
    let (sender, kp, fx) = publish(&mut cluster, "burn_only").await;
    let package = find_immutable(&fx).unwrap().0;
    let currency = find_address_owned_by(&fx, SUI_COIN_REGISTRY_ADDRESS.into()).unwrap();
    let coin = find_address_owned_by(&fx, sender).unwrap();
    let gas = fx.gas_object().0;

    // Finalize the registration -- the `Currency<T>` is re-created as a shared object with a
    // derived address by this operation.
    let fx = finalize(&mut cluster, sender, &kp, package, "burn", currency, gas).await;
    let currency = find_shared(&fx).unwrap();
    let gas = fx.gas_object().0;

    // Generate and index checkpoint
    cluster.create_checkpoint().await;

    // Query the initial coin metadata
    let metadata = query_metadata(&cluster, &format!("{package}::burn::BURN")).await;
    assert_debug_snapshot!(metadata, @r###"
    CoinMetadata {
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
    }
    "###);

    // Split and burn some of the coin
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

    // Generate and index checkpoint after burn
    cluster.create_checkpoint().await;

    // Query the coin metadata after burn
    let metadata = query_metadata(&cluster, &format!("{package}::burn::BURN")).await;
    assert_debug_snapshot!(metadata, @r###"
    CoinMetadata {
        name: "Burn",
        decimals: 2,
        description: "A fake burn-only coin for test purposes",
        icon_url: Some(
            "https://example.com/fake.png",
        ),
        supply: Some(
            "900000000",
        ),
        symbol: "BURN",
    }
    "###);
}

#[tokio::test]
async fn test_unknown() {
    let mut cluster = FullCluster::new().await.unwrap();

    // Publish the unknown coin (with shared treasury cap)
    let (sender, kp, fx) = publish(&mut cluster, "unknown").await;
    let package = find_immutable(&fx).unwrap().0;
    let currency = find_address_owned_by(&fx, SUI_COIN_REGISTRY_ADDRESS.into()).unwrap();
    let coin = find_address_owned_by(&fx, sender).unwrap();
    let treasury_cap = find_shared(&fx).unwrap(); // Treasury cap is now shared
    let gas = fx.gas_object().0;

    // Finalize the registration
    let fx = finalize(&mut cluster, sender, &kp, package, "unknown", currency, gas).await;
    let gas = fx.gas_object().0;

    // Generate and index checkpoint
    cluster.create_checkpoint().await;

    // Query the initial coin metadata
    let metadata = query_metadata(&cluster, &format!("{package}::unknown::UNKNOWN")).await;
    assert_debug_snapshot!(metadata, @r###"
    CoinMetadata {
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
    }
    "###);

    // Burn some of the coin using the treasury cap
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
    let gas = fx.gas_object().0;
    let coin = find_address_mutated(&fx).unwrap();

    // Generate and index checkpoint after burn
    cluster.create_checkpoint().await;

    // Query the coin metadata after burn
    let metadata = query_metadata(&cluster, &format!("{package}::unknown::UNKNOWN")).await;
    assert_debug_snapshot!(metadata, @r###"
    CoinMetadata {
        name: "Unknown",
        decimals: 2,
        description: "A fake unknown treasury coin for test purposes",
        icon_url: Some(
            "https://example.com/unknown.png",
        ),
        supply: Some(
            "800000000",
        ),
        symbol: "UNKNOWN",
    }
    "###);

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
    let gas = fx.gas_object().0;

    // Generate and index checkpoint after hiding
    cluster.create_checkpoint().await;

    // Query metadata after hiding - supply should be None
    let metadata = query_metadata(&cluster, &format!("{package}::unknown::UNKNOWN")).await;
    assert_debug_snapshot!(metadata, @r###"
    CoinMetadata {
        name: "Unknown",
        decimals: 2,
        description: "A fake unknown treasury coin for test purposes",
        icon_url: Some(
            "https://example.com/unknown.png",
        ),
        supply: None,
        symbol: "UNKNOWN",
    }
    "###);

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
    let gas = fx.gas_object().0;

    // Generate and index checkpoint after burning while hidden
    cluster.create_checkpoint().await;

    // Query metadata after burning while hidden - supply should still be None
    let metadata = query_metadata(&cluster, &format!("{package}::unknown::UNKNOWN")).await;
    assert_debug_snapshot!(metadata, @r###"
    CoinMetadata {
        name: "Unknown",
        decimals: 2,
        description: "A fake unknown treasury coin for test purposes",
        icon_url: Some(
            "https://example.com/unknown.png",
        ),
        supply: None,
        symbol: "UNKNOWN",
    }
    "###);

    // Show the treasury cap again
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

    // Generate and index checkpoint after showing
    cluster.create_checkpoint().await;

    // Query metadata after showing - supply should reflect both burns (700M remaining)
    let metadata = query_metadata(&cluster, &format!("{package}::unknown::UNKNOWN")).await;
    assert_debug_snapshot!(metadata, @r###"
    CoinMetadata {
        name: "Unknown",
        decimals: 2,
        description: "A fake unknown treasury coin for test purposes",
        icon_url: Some(
            "https://example.com/unknown.png",
        ),
        supply: Some(
            "700000000",
        ),
        symbol: "UNKNOWN",
    }
    "###);
}

#[tokio::test]
async fn test_legacy() {
    let mut cluster = FullCluster::new().await.unwrap();

    // Publish the legacy coin
    let (sender, kp, fx) = publish(&mut cluster, "legacy").await;
    let package = find_immutable(&fx).unwrap().0;
    let treasury_cap = find_shared(&fx).unwrap();
    let coin_metadata = find_address_owned_by(&fx, sender).unwrap();
    let gas = fx.gas_object().0;

    // Generate and index checkpoint
    cluster.create_checkpoint().await;

    // Query the coin metadata for the legacy coin (before migration)
    let metadata = query_metadata(&cluster, &format!("{package}::legacy::LEGACY")).await;
    assert_debug_snapshot!(metadata, @r###"
    CoinMetadata {
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
    }
    "###);

    // Migrate the legacy coin to the coin registry
    let fx = migrate(
        &mut cluster,
        sender,
        &kp,
        package,
        "legacy",
        coin_metadata,
        gas,
    )
    .await;
    let currency = find_shared(&fx).unwrap(); // The migrated Currency<T> object
    let gas = fx.gas_object().0;

    // Generate and index checkpoint after migration
    cluster.create_checkpoint().await;

    // Query the coin metadata again after migration - should produce the same results
    let metadata = query_metadata(&cluster, &format!("{package}::legacy::LEGACY")).await;
    assert_debug_snapshot!(metadata, @r###"
    CoinMetadata {
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
    }
    "###);

    // Clean up the legacy metadata
    delete_migrated_legacy_metadata(
        &mut cluster,
        sender,
        &kp,
        package,
        "legacy",
        currency,
        treasury_cap,
        coin_metadata,
        gas,
    )
    .await;

    // Generate and index checkpoint after cleanup
    cluster.create_checkpoint().await;

    // Query the coin metadata again after cleanup - should still be readable
    let metadata = query_metadata(&cluster, &format!("{package}::legacy::LEGACY")).await;
    assert_debug_snapshot!(metadata, @r###"
    CoinMetadata {
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
    }
    "###);
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
/// - `package` and `module` identify the coin type `T` as `<package>::<module>::<MODULE>`.
/// - `metadata` is the shared CoinMetadata<T> object.
/// - `gas` is the gas object to use for the transaction, which must be owned by `sender`.
///
/// Returns the effects of running the migration transaction.
async fn migrate(
    cluster: &mut FullCluster,
    sender: SuiAddress,
    kp: &Ed25519KeyPair,
    package: ObjectID,
    module: &str,
    metadata: ObjectRef,
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

    let metadata = builder.obj(ObjectArg::ImmOrOwnedObject(metadata)).unwrap();

    builder.programmable_move_call(
        SUI_FRAMEWORK_PACKAGE_ID,
        ident_str!("coin_registry").to_owned(),
        ident_str!("migrate_legacy_metadata").to_owned(),
        vec![TypeTag::Struct(Box::new(StructTag {
            address: package.into(),
            module: Identifier::new(module).unwrap(),
            name: Identifier::new(module.to_owned().to_ascii_uppercase()).unwrap(),
            type_params: vec![],
        }))],
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

/// Delete the migrated legacy metadata.
///
/// - `sender` and `kp` describe the account that will sign and pay for the transaction.
/// - `package` and `module` identify the coin type `T` as `<package>::<module>::<MODULE>`.
/// - `currency` is the `Currency<T>` object.
/// - `treasury_cap` is the `TreasuryCap<T>` object.
/// - `metadata` is the `CoinMetadata<T>` object to delete.
/// - `gas` is the gas object to use for the transaction, which must be owned by `sender`.
///
/// Returns the effects of running the delete transaction.
async fn delete_migrated_legacy_metadata(
    cluster: &mut FullCluster,
    sender: SuiAddress,
    kp: &Ed25519KeyPair,
    package: ObjectID,
    module: &str,
    currency: ObjectRef,
    treasury_cap: ObjectRef,
    metadata: ObjectRef,
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
        .obj(ObjectArg::SharedObject {
            id: treasury_cap.0,
            initial_shared_version: treasury_cap.1,
            mutable: false,
        })
        .unwrap();

    let legacy_metadata = builder.obj(ObjectArg::ImmOrOwnedObject(metadata)).unwrap();

    let coin_type = TypeTag::Struct(Box::new(StructTag {
        address: package.into(),
        module: Identifier::new(module).unwrap(),
        name: Identifier::new(module.to_owned().to_ascii_uppercase()).unwrap(),
        type_params: vec![],
    }));

    // Claim the metadata cap
    let metadata_cap = builder.programmable_move_call(
        SUI_FRAMEWORK_PACKAGE_ID,
        ident_str!("coin_registry").to_owned(),
        ident_str!("claim_metadata_cap").to_owned(),
        vec![coin_type.clone()],
        vec![currency, treasury_cap],
    );

    // Delete the metadata cap
    builder.programmable_move_call(
        SUI_FRAMEWORK_PACKAGE_ID,
        ident_str!("coin_registry").to_owned(),
        ident_str!("delete_metadata_cap").to_owned(),
        vec![coin_type.clone()],
        vec![currency, metadata_cap],
    );

    // Delete the migrated legacy metadata
    builder.programmable_move_call(
        SUI_FRAMEWORK_PACKAGE_ID,
        ident_str!("coin_registry").to_owned(),
        ident_str!("delete_migrated_legacy_metadata").to_owned(),
        vec![coin_type],
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
async fn query_metadata(cluster: &FullCluster, coin_type: &str) -> CoinMetadata {
    let client = reqwest::Client::new();
    let url = cluster.graphql_url();

    let response: serde_json::Value = client
        .post(url.as_str())
        .json(&json!({
            "query": QUERY,
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
