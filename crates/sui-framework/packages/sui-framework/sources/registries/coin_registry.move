// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

/// Defines the system object for managing coin data in a central
/// registry. This module provides a centralized way to store and manage
/// metadata for all coin types in the Sui ecosystem, including their
/// supply information, regulatory status, and metadata capabilities.
module sui::coin_registry;
use std::string::String;
use std::type_name::TypeName;
use sui::balance::Supply;
use sui::dynamic_object_field;
use sui::transfer::Receiving;
use sui::vec_map::{Self, VecMap};

// Allows calling `.add_dof(object, name, value)` on `UID`
use fun dynamic_object_field::add as UID.add_dof;
// Allows calling `.borrow_dof(object, name)` on `UID`
use fun dynamic_object_field::borrow as UID.borrow_dof;
// Allows calling `.borrow_dof_mut(object, name)` on `UID`
use fun dynamic_object_field::borrow_mut as UID.borrow_dof_mut;
// Allows calling `.exists_dof(object, name)` on `UID`
use fun dynamic_object_field::exists_ as UID.exists_dof;

/// No CoinData found for this coin type.
const ECoinDataNotFound: u64 = 0;
/// Metadata cap already claimed
const EMetadataCapAlreadyClaimed: u64 = 1;
/// Only the system address can create the registry
const ENotSystemAddress: u64 = 2;
/// CoinData for this coin type already exists
const ECoinDataAlreadyExists: u64 = 3;

/// Variant identifier for regulated coins in the deny list
const REGULATED_COIN_VARIANT: u8 = 0;

/// System object found at address 0xc that stores coin data for all
/// registered coin types. This is a shared object that acts as a central
/// registry for coin metadata, supply information, and regulatory status.
public struct CoinRegistry has key, store {
    id: UID,
}

/// Store only object that enables more flexible coin data
/// registration, allowing for additional fields to be added
/// without changing the CoinData structure.
#[allow(unused_field)]
public struct ExtraField(TypeName, vector<u8>) has store;

/// Key used to access coin metadata hung off the `CoinRegistry`
/// object. This key can be versioned to allow for future changes
/// to the metadata object.
public struct CoinDataKey<phantom T>() has copy, drop, store;

/// Capability object that enables coin metadata to be updated.
/// This capability is created when a coin is registered and allows
/// the holder to modify the coin's metadata fields.
public struct MetadataCap<phantom T> has key, store { id: UID }

/// CoinData object that stores comprehensive information about a coin type.
/// This includes metadata like name, symbol, and description, as well as
/// supply and regulatory status information.
public struct CoinData<phantom T> has key, store {
    id: UID,
    /// Number of decimal places the coin uses for display purposes
    decimals: u8,
    /// Human-readable name for the token
    name: String,
    /// Short symbol/ticker for the token
    symbol: String,
    /// Detailed description of the token
    description: String,
    /// URL for the token's icon/logo
    icon_url: String,
    /// Current supply state of the coin (fixed supply or unknown)
    supply: Option<SupplyState<T>>,
    /// Regulatory status of the coin (regulated with deny cap or unknown)
    regulated: RegulatedState,
    /// ID of the treasury cap for this coin type, if registered
    treasury_cap_id: Option<ID>,
    /// ID of the metadata capability for this coin type, if claimed
    metadata_cap_id: Option<ID>,
    /// Additional fields for extensibility
    extra_fields: VecMap<String, ExtraField>,
}

/// Supply state of a coin type, which can be fixed (with a known supply)
/// or unknown (supply not yet registered in the registry).
public enum SupplyState<phantom T> has store {
    /// Coin has a fixed supply with the given Supply object
    Fixed(Supply<T>),
    /// Supply information is not yet known or registered
    Unknown,
}

/// Regulated state of a coin type, which can be regulated with a deny cap
/// for address restrictions, or unknown if not regulated.
public enum RegulatedState has copy, drop, store {
    /// Coin is regulated with a deny cap for address restrictions
    Regulated { cap: ID, variant: u8 },
    /// Coin is not regulated or regulatory status is unknown
    Unknown,
}

/// Hot potato pattern object to enforce registration after "create_currency" data creation.
/// This object must be transferred to the registry to complete the coin registration process.
public struct InitCoinData<phantom T> {
    data: CoinData<T>,
}

/// Return the ID of the system coin registry object located at address 0xc.
public fun coin_registry_id(): ID {
    @0xc.to_id()
}

/// Get the ID of the registry object.
public fun id(registry: &CoinRegistry): ID {
    registry.id.to_inner()
}

/// Transfer the InitCoinData to the registry to complete coin registration.
/// This function is called after `create_currency_v2` to register the coin data
/// in the central registry.
public fun transfer_to_registry<T>(init: InitCoinData<T>) {
    let InitCoinData { data } = init;

    transfer::transfer(
        data,
        coin_registry_id().to_address(),
    );
}

/// Enables CoinData to be registered in the `CoinRegistry` object
/// via TTO (Transfer To Object) pattern.
public fun migrate_receiving<T>(
    registry: &mut CoinRegistry,
    coin_data: Receiving<CoinData<T>>,
) {
    let received_data = transfer::public_receive(&mut registry.id, coin_data);
    registry.register_coin_data(received_data);
}

// === CoinData Setters  ===

/// Enables a metadata cap holder to update a coin's name.
public fun set_name<T>(
    registry: &mut CoinRegistry,
    _: &MetadataCap<T>,
    name: String,
) {
    registry.data_mut<T>().name = name;
}

/// Enables a metadata cap holder to update a coin's symbol.
public fun set_symbol<T>(
    registry: &mut CoinRegistry,
    _: &MetadataCap<T>,
    symbol: String,
) {
    registry.data_mut<T>().symbol = symbol;
}

/// Enables a metadata cap holder to update a coin's description.
public fun set_description<T>(
    registry: &mut CoinRegistry,
    _: &MetadataCap<T>,
    description: String,
) {
    registry.data_mut<T>().description = description;
}

/// Enables a metadata cap holder to update a coin's icon URL.
public fun set_icon_url<T>(
    registry: &mut CoinRegistry,
    _: &MetadataCap<T>,
    icon_url: String,
) {
    registry.data_mut<T>().icon_url = icon_url;
}

/// Get immutable reference to the coin data for type T.
/// Aborts if no coin data exists for this type.
public fun data<T>(registry: &CoinRegistry): &CoinData<T> {
    assert!(registry.exists<T>(), ECoinDataNotFound);
    registry.id.borrow_dof(CoinDataKey<T>())
}

// === Public getters  ===

/// Get the number of decimal places for the coin type.
public fun decimals<T>(coin_data: &CoinData<T>): u8 { coin_data.decimals }

/// Get the human-readable name of the coin.
public fun name<T>(coin_data: &CoinData<T>): String { coin_data.name }

/// Get the symbol/ticker of the coin.
public fun symbol<T>(coin_data: &CoinData<T>): String { coin_data.symbol }

/// Get the description of the coin.
public fun description<T>(coin_data: &CoinData<T>): String {
    coin_data.description
}

/// Get the icon URL for the coin.
public fun icon_url<T>(coin_data: &CoinData<T>): String { coin_data.icon_url }

/// Check if the metadata capability has been claimed for this coin type.
public fun meta_data_cap_claimed<T>(coin_data: &CoinData<T>): bool {
    coin_data.metadata_cap_id.is_some()
}

/// Get the treasury cap ID for this coin type, if registered.
public fun treasury_cap<T>(coin_data: &CoinData<T>): Option<ID> {
    coin_data.treasury_cap_id
}

/// Get the deny cap ID for this coin type, if it's a regulated coin.
public fun deny_cap<T>(coin_data: &CoinData<T>): Option<ID> {
    match (coin_data.regulated) {
        RegulatedState::Regulated { cap, .. } => option::some(cap),
        RegulatedState::Unknown => option::none(),
    }
}

/// Check if the supply has been registered for this coin type.
public fun supply_registered<T>(coin_data: &CoinData<T>): bool {
    match (coin_data.supply.borrow()) {
        SupplyState::Fixed(_) => true,
        SupplyState::Unknown => false,
    }
}

/// Check if coin data exists for the given type T in the registry.
public fun exists<T>(registry: &CoinRegistry): bool {
    registry.id.exists_dof(CoinDataKey<T>())
}

/// Get immutable reference to the coin data from InitCoinData.
public fun inner<T>(init: &InitCoinData<T>): &CoinData<T> {
    &init.data
}

// === Internal registration functions  ===

/// Register the supply for a coin type in the registry.
/// This function is package-private and should only be called by the coin module.
/// Aborts if no coin data exists for this type or if supply is already registered.
public(package) fun register_supply<T>(
    registry: &mut CoinRegistry,
    supply: Supply<T>,
) {
    assert!(registry.exists<T>(), ECoinDataNotFound);
    registry.data_mut<T>().treasury_cap_id.extract();
    match (registry.data_mut<T>().supply.swap(SupplyState::Fixed(supply))) {
        SupplyState::Fixed(_supply) => abort ,
        SupplyState::Unknown => (),
    };
}

/// Register a coin type as regulated with the given deny cap ID.
/// This function is package-private and should only be called by the coin module.
/// Aborts if no coin data exists for this type.
public(package) fun register_regulated<T>(
    registry: &mut CoinRegistry,
    deny_cap_id: ID,
) {
    assert!(registry.exists<T>(), ECoinDataNotFound);
    registry.data_mut<T>().regulated =
        RegulatedState::Regulated {
            cap: deny_cap_id,
            variant: REGULATED_COIN_VARIANT,
        };
}

/// Register the treasury cap ID for a coin type.
/// This function is package-private and should only be called by the coin module.
public(package) fun register_treasury_cap<T>(
    registry: &mut CoinRegistry,
    cap_id: &ID,
) {
    registry.data_mut<T>().treasury_cap_id.fill(*cap_id);
}

/// Set the decimals for a coin data object.
/// This function is package-private and should only be called by the coin module.
public(package) fun set_decimals<T>(data: &mut CoinData<T>, decimals: u8) {
    data.decimals = decimals;
}

/// Set the supply for a coin data object.
/// This function is package-private and should only be called by the coin module.
/// Aborts if supply is already set.
public(package) fun set_supply<T>(data: &mut CoinData<T>, supply: Supply<T>) {
    match (data.supply.swap(SupplyState::Fixed(supply))) {
        SupplyState::Fixed(_supply) => abort ,
        SupplyState::Unknown => (),
    };
}

/// Set a coin as regulated with the given deny cap ID.
/// This function is package-private and should only be called by the coin module.
public(package) fun set_regulated<T>(data: &mut CoinData<T>, deny_cap_id: ID) {
    data.regulated =
        RegulatedState::Regulated {
            cap: deny_cap_id,
            variant: REGULATED_COIN_VARIANT,
        };
}

/// Get mutable reference to the coin data for type T.
/// This function is package-private and should only be called by the coin module.
/// Aborts if no coin data exists for this type.
public(package) fun data_mut<T>(registry: &mut CoinRegistry): &mut CoinData<T> {
    assert!(registry.exists<T>(), ECoinDataNotFound);
    registry.id.borrow_dof_mut(CoinDataKey<T>())
}

/// Register coin data for a new coin type in the registry.
/// This function is package-private and should only be called by the coin module.
/// Aborts if coin data already exists for this type.
public(package) fun register_coin_data<T>(
    registry: &mut CoinRegistry,
    data: CoinData<T>,
) {
    assert!(!registry.exists<T>(), ECoinDataAlreadyExists);

    registry.id.add_dof(CoinDataKey<T>(), data);
}

/// Get mutable reference to the coin data from InitCoinData.
/// This function is package-private and should only be called by the coin module.
public(package) fun inner_mut<T>(init: &mut InitCoinData<T>): &mut CoinData<T> {
    &mut init.data
}

/// Create an InitCoinData object with the specified parameters.
/// This function is package-private and should only be called by the coin module.
public(package) fun create_coin_data_init<T>(
    decimals: u8,
    name: String,
    symbol: String,
    description: String,
    icon_url: String,
    supply: Option<Supply<T>>,
    treasury_cap_id: Option<ID>,
    metadata_cap_id: Option<ID>,
    deny_cap_id: Option<ID>,
    ctx: &mut TxContext,
): InitCoinData<T> {
    InitCoinData {
        data: create_coin_data(
            decimals,
            name,
            symbol,
            description,
            icon_url,
            supply,
            treasury_cap_id,
            metadata_cap_id,
            deny_cap_id,
            ctx,
        ),
    }
}

/// Create a new CoinData object with the specified parameters.
/// This function is package-private and should only be called by the coin module.
public(package) fun create_coin_data<T>(
    decimals: u8,
    name: String,
    symbol: String,
    description: String,
    icon_url: String,
    supply: Option<Supply<T>>,
    treasury_cap_id: Option<ID>,
    metadata_cap_id: Option<ID>,
    deny_cap_id: Option<ID>,
    ctx: &mut TxContext,
): CoinData<T> {
    let supply = supply
        .map!(|supply| SupplyState::Fixed(supply))
        .or!(option::some(SupplyState::Unknown));

    let regulated_state = deny_cap_id
        .map!(
            |cap| RegulatedState::Regulated {
                cap,
                variant: REGULATED_COIN_VARIANT,
            },
        )
        .destroy_or!(RegulatedState::Unknown);

    CoinData {
        id: object::new(ctx),
        decimals,
        name,
        symbol,
        description,
        icon_url,
        supply,
        regulated: regulated_state,
        treasury_cap_id,
        metadata_cap_id,
        extra_fields: vec_map::empty(),
    }
}

/// Create an empty CoinData object for testing purposes.
/// This function is package-private and should only be called by the coin module.
public(package) fun empty<T>(ctx: &mut TxContext): CoinData<T> {
    CoinData {
        id: object::new(ctx),
        decimals: 0,
        name: b"".to_string(),
        symbol: b"".to_string(),
        description: b"".to_string(),
        icon_url: b"".to_string(),
        regulated: RegulatedState::Unknown,
        supply: option::some(SupplyState::Unknown),
        treasury_cap_id: option::none(),
        metadata_cap_id: option::none(),
        extra_fields: vec_map::empty(),
    }
}

/// Create a MetadataCap for a coin type.
/// This function is package-private and should only be called by the coin module.
/// Aborts if the metadata capability has already been claimed.
public(package) fun create_cap<T>(
    data: &mut CoinData<T>,
    ctx: &mut TxContext,
): MetadataCap<T> {
    assert!(!data.meta_data_cap_claimed(), EMetadataCapAlreadyClaimed);
    let id = object::new(ctx);
    let metadata_cap_id = id.to_inner();

    data.metadata_cap_id.fill(metadata_cap_id);

    MetadataCap { id }
}

#[allow(unused_function)]
/// Create and share the singleton Registry -- this function is
/// called exactly once, during the upgrade epoch.
/// Only the system address (0x0) can create the registry.
fun create(ctx: &TxContext) {
    assert!(ctx.sender() == @0x0, ENotSystemAddress);

    transfer::share_object(CoinRegistry {
        id: object::sui_coin_registry_object_id(),
    });
}

#[test_only]
/// Create a coin data registry for testing purposes.
/// This function is test-only and should only be used in tests.
public fun create_coin_data_registry_for_testing(
    ctx: &mut TxContext,
): CoinRegistry {
    assert!(ctx.sender() == @0x0, ENotSystemAddress);

    CoinRegistry {
        id: object::new(ctx),
    }
}

#[test_only]
/// Unwrap InitCoinData for testing purposes.
/// This function is test-only and should only be used in tests.
public fun unwrap_for_testing<T>(init: InitCoinData<T>): CoinData<T> {
    let InitCoinData { data } = init;
    data
}
