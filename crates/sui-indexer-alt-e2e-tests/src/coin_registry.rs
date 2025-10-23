// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

use std::path::PathBuf;

use fastcrypto::ed25519::Ed25519KeyPair;
use move_core_types::{ident_str, language_storage::StructTag};
use sui_move_build::BuildConfig;
use sui_types::{
    Identifier, SUI_COIN_REGISTRY_OBJECT_ID, SUI_FRAMEWORK_PACKAGE_ID, TypeTag,
    base_types::{ObjectID, ObjectRef, SuiAddress},
    effects::TransactionEffects,
    programmable_transaction_builder::ProgrammableTransactionBuilder,
    transaction::{
        CallArg, Command, ObjectArg, SharedObjectMutability, Transaction, TransactionData,
    },
};

use crate::FullCluster;

/// 5 SUI gas budget
pub const DEFAULT_GAS_BUDGET: u64 = 5_000_000_000;

/// Output from querying owned objects related to creating a new legacy currency.
pub struct LegacyCoinOutputs {
    pub coin_type: StructTag,
    pub treasury: ObjectRef,
    pub metadata: Option<ObjectRef>,
    pub deny: Option<ObjectRef>,
}

/// Publish `packages/coin_registry/<package>` to `cluster` with a fresh, funded account.
///
/// Returns the address and keypair of the publishing address, and the effects of the publish
/// transaction.
pub async fn publish(
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
pub async fn finalize(
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
                    mutability: SharedObjectMutability::Mutable,
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

/// Create a new dynamic currency by calling the coin's module's `new_currency` function (This is
/// specifically designed to work with the `dynamic` test package).
///
/// - `cluster` - The test cluster to execute the transaction on
/// - `sender` and `kp` - The account that will sign and pay for the transaction
/// - `coin_type` - The full struct tag for the coin type (contains package, module, and type name)
/// - `gas` - The gas object to use for the transaction
///
/// Returns the effects of running the new_currency transaction.
pub async fn create_dynamic_currency(
    cluster: &mut FullCluster,
    sender: SuiAddress,
    kp: &Ed25519KeyPair,
    coin_type: StructTag,
    gas: ObjectRef,
) -> TransactionEffects {
    let mut builder = ProgrammableTransactionBuilder::new();

    builder
        .move_call(
            coin_type.address.into(),
            coin_type.module.clone(),
            ident_str!("new_currency").to_owned(),
            vec![],
            vec![CallArg::Object(ObjectArg::SharedObject {
                id: SUI_COIN_REGISTRY_OBJECT_ID,
                initial_shared_version: 1.into(),
                mutability: SharedObjectMutability::Mutable,
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

    let (fx, error) = cluster
        .execute_transaction(Transaction::from_data_and_signer(data, vec![kp]))
        .expect("new_currency failed");

    assert!(error.is_none(), "new_currency failed: {error:?}");
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
pub async fn burn_from_currency(
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
            mutability: SharedObjectMutability::Mutable,
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
pub async fn burn_from_treasury(
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
            mutability: SharedObjectMutability::Mutable,
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
pub async fn hide_treasury_cap(
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
            mutability: SharedObjectMutability::Mutable,
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
pub async fn show_treasury_cap(
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
            mutability: SharedObjectMutability::Mutable,
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
pub async fn migrate(
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
            mutability: SharedObjectMutability::Mutable,
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
pub async fn migrate_deny_cap(
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
            mutability: SharedObjectMutability::Mutable,
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
pub async fn migrate_regulated_metadata(
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
            mutability: SharedObjectMutability::Mutable,
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
pub async fn delete_migrated_legacy_metadata(
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
            mutability: SharedObjectMutability::Mutable,
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
