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
use sui::coin::{Self, TreasuryCap, DenyCapV2, CoinMetadata, RegulatedCoinMetadata};
use sui::transfer::Receiving;
use sui::vec_map::{Self, VecMap};

/// No CoinData found for this coin type.
const ECoinDataNotFound: u64 = 0;
/// Metadata cap already claimed
const EMetadataCapAlreadyClaimed: u64 = 1;
/// Only the system address can create the registry
const ENotSystemAddress: u64 = 2;
/// CoinData for this coin type already exists
const ECoinDataAlreadyExists: u64 = 3;
/// Attempt to set the deny list state permissionlessly while it has already been set.
const EDenyListStateAlreadySet: u64 = 4;
/// Tries to delete legacy metadata without having claimed the management capability.
const EMetadataCapNotClaimed: u64 = 5;
///
const ECannotUpdateManagedMetadata: u64 = 6;

/// Incremental identifier for regulated coin versions in the deny list.
/// 0 here matches DenyCapV2 world.
/// TODO: Fix wording here.
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
public struct CoinData<phantom T> has key {
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
    /// Note: We're using `Option` because `SupplyState` does not have drop,
    /// meaning we cannot swap out its value at a later state.
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
    Frozen(Supply<T>),
    /// Coin has a supply that can ONLY decrease.
    /// TODO: Public burn function OR capability? :)
    Deflationary(Supply<T>),
    /// Supply information is not yet known or registered
    Unknown,
}

/// Regulated state of a coin type, which can be regulated with a deny cap
/// for address restrictions, or unknown if not regulated.
public enum RegulatedState has copy, drop, store {
    /// Coin is regulated with a deny cap for address restrictions
    Regulated { cap: ID, variant: u8 },
    /// The coin has been created without deny list
    Unregulated,
    /// Coin is not regulated or regulatory status is unknown.
    /// This is the result of a legacy migration for that coin (from `coin.move` constructors)
    Unknown,
}

/// Hot potato pattern object to enforce registration after "create_currency" data creation.
/// This object must be transferred to the registry to complete the coin registration process.
public struct InitCoinData<phantom T> {
    data: CoinData<T>,
}

// 1. Entrypoint for creating currency [done]
// 2. Entrypoint for creating regulated currency [done]
// 3. Claim capability (using treasury cap) [done]
// 3. Migrate existing CoinMetadada -> Registry Metadata
// 4.
public fun register_currency<T: drop>(
    otw: T,
    decimals: u8,
    symbol: String,
    name: String,
    description: String,
    icon_url: String,
    ctx: &mut TxContext,
): (TreasuryCap<T>, MetadataCap<T>, InitCoinData<T>) {
    // Make sure there's only one instance of the type T
    assert!(sui::types::is_one_time_witness(&otw));

    let treasury_cap = coin::new_treasury_cap(otw, ctx);

    let mut metadata = CoinData<T> {
        id: object::new(ctx),
        decimals,
        name,
        symbol,
        description,
        icon_url,
        supply: option::some(SupplyState::Unknown),
        regulated: RegulatedState::Unregulated,
        treasury_cap_id: option::some(object::id(&treasury_cap)),
        metadata_cap_id: option::none(),
        extra_fields: vec_map::empty(),
    };

    let metadata_cap = metadata.claim_cap(&treasury_cap, ctx);

    (treasury_cap, metadata_cap, InitCoinData { data: metadata })
}

/// Allows converting an `OTW` coin into a regulated coin.
public fun make_regulated<T>(
    init: &mut InitCoinData<T>,
    allow_global_pause: bool,
    ctx: &mut TxContext,
): DenyCapV2<T> {
    assert!(init.data.regulated == RegulatedState::Unregulated);
    let deny_cap = coin::new_deny_cap_v2<T>(allow_global_pause, ctx);

    init.inner_mut().regulated =
        RegulatedState::Regulated {
            cap: object::id(&deny_cap),
            variant: REGULATED_COIN_VARIANT,
        };

    deny_cap
}

/// Claim a MetadataCap for a coin type. This is only allowed from the owner of `TreasuryCap`, and only once.
/// Aborts if the metadata capability has already been claimed.
public fun claim_cap<T>(
    data: &mut CoinData<T>,
    _: &TreasuryCap<T>,
    ctx: &mut TxContext,
): MetadataCap<T> {
    assert!(!data.meta_data_cap_claimed(), EMetadataCapAlreadyClaimed);
    let id = object::new(ctx);
    let metadata_cap_id = id.to_inner();

    data.metadata_cap_id.fill(metadata_cap_id);

    MetadataCap { id }
}

/// Freeze the supply by destroying the TreasuryCap and storing it in the CoinData.
public fun freeze_supply<T>(data: &mut CoinData<T>, cap: TreasuryCap<T>) {
    match (data.supply.swap(SupplyState::Frozen(cap.into_supply()))) {
        // Impossible: We cannot fix a supply or make a supply deflationary twice.
        SupplyState::Frozen(_supply) | SupplyState::Deflationary(_supply) => abort,
        // We replaced "unknown" with fixed supply.
        SupplyState::Unknown => (),
    };
}

/// Make the supply "deflatinary" by destroying the TreasuryCap and taking control of the
/// supply through the CoinData.
public fun make_deflationary<T>(data: &mut CoinData<T>, cap: TreasuryCap<T>) {
    match (data.supply.swap(SupplyState::Deflationary(cap.into_supply()))) {
        // Impossible: We cannot fix a supply or make a supply deflationary twice.
        SupplyState::Frozen(_supply) | SupplyState::Deflationary(_supply) => abort,
        // We replaced "unknown" with frozen supply.
        SupplyState::Unknown => (),
    };
}

/// Transfer the InitCoinData to the registry to complete coin registration.
/// This function is called after `register_currency` to register the coin data
/// in the central registry.
public fun transfer_to_registry<T>(init: InitCoinData<T>) {
    let InitCoinData { data } = init;

    transfer::transfer(
        data,
        coin_registry_id().to_address(),
    );
}

/// The second step in the "otw" initialization of coin metadata, that takes in the `CoinData<T>` that was
/// transferred from init, and transforms it in to a "derived address" shared object.
public fun finalize_registration<T>(
    registry: &mut CoinRegistry,
    coin_data: Receiving<CoinData<T>>,
    ctx: &mut TxContext,
) {
    // 1. Consume CoinData
    // 2. Re-create it with a "derived" address.
    let CoinData {
        id,
        decimals,
        name,
        symbol,
        description,
        icon_url,
        supply,
        regulated,
        treasury_cap_id,
        metadata_cap_id,
        extra_fields,
    } = transfer::receive(&mut registry.id, coin_data);

    id.delete();

    // Now, create the shared version of the coin data.
    transfer::share_object(CoinData {
        // TODO: Replace this with `derived_object::claim()`
        id: object::new(ctx),
        decimals,
        name,
        symbol,
        description,
        icon_url,
        supply,
        regulated,
        treasury_cap_id,
        metadata_cap_id,
        extra_fields,
    })
}

/// Get mutable reference to the coin data from InitCoinData.
/// This function is package-private and should only be called by the coin module.
public fun inner_mut<T>(init: &mut InitCoinData<T>): &mut CoinData<T> {
    &mut init.data
}

// === CoinData Setters  ===

/// Enables a metadata cap holder to update a coin's name.
public fun set_name<T>(data: &mut CoinData<T>, _: &MetadataCap<T>, name: String) {
    data.name = name;
}

/// Enables a metadata cap holder to update a coin's symbol.
/// TODO: Should we kill this? :)
public fun set_symbol<T>(data: &mut CoinData<T>, _: &MetadataCap<T>, symbol: String) {
    data.symbol = symbol;
}

/// Enables a metadata cap holder to update a coin's description.
public fun set_description<T>(data: &mut CoinData<T>, _: &MetadataCap<T>, description: String) {
    data.description = description;
}

/// Enables a metadata cap holder to update a coin's icon URL.
public fun set_icon_url<T>(data: &mut CoinData<T>, _: &MetadataCap<T>, icon_url: String) {
    data.icon_url = icon_url;
}

/// Register the treasury cap ID for a coin type at a later point.
public fun set_treasury_cap_id<T>(data: &mut CoinData<T>, cap: &TreasuryCap<T>) {
    data.treasury_cap_id.fill(object::id(cap));
}

// == Migrations from legacy coin flows ==

/// TODO: Register legacy coin metadata to the registry --
/// This should:
/// 1. Take the old metadata
/// 2. Create a `CoinData<T>` object with a derived address (and share it!)
public fun migrate_legacy_metadata<T>(registry: &mut CoinRegistry, v1: &CoinMetadata<T>) {
    abort
}

/// TODO: Allow coin metadata to be updated from legacy as described in our docs.
public fun update_from_legacy_metadata<T>(data: &mut CoinData<T>, v1: &CoinMetadata<T>) {
    assert!(!data.meta_data_cap_claimed(), ECannotUpdateManagedMetadata);
    abort
}

/// Delete the legacy `CoinMetadata` object if the metadata cap for the new registry
/// has already been claimed.
///
/// This function is only callable after there's "proof" that the author of the coin
/// can manage the metadata using the registry system (so having a metadata cap claimed).
public fun delete_migrated_legacy_metadata<T>(data: &mut CoinData<T>, v1: CoinMetadata<T>) {
    assert!(data.meta_data_cap_claimed(), EMetadataCapNotClaimed);
    v1.destroy_metadata();
}

/// Allow migrating the regulated state by access to `RegulatedCoinMetadata` frozen object.
/// This is a permissionless operation.
public fun migrate_regulated_state_by_metadata<T>(
    data: &mut CoinData<T>,
    metadata: &RegulatedCoinMetadata<T>,
) {
    // Only allow if this hasn't been migrated before.
    assert!(data.regulated == RegulatedState::Unknown, EDenyListStateAlreadySet);
    data.regulated =
        RegulatedState::Regulated {
            cap: metadata.deny_cap_id(),
            variant: REGULATED_COIN_VARIANT,
        };
}

/// Allow migrating the regulated state by a `DenyCapV2` object.
/// This is a permissioned operation.
public fun migrate_regulated_state_by_cap<T>(data: &mut CoinData<T>, cap: &DenyCapV2<T>) {
    data.regulated =
        RegulatedState::Regulated {
            cap: object::id(cap),
            variant: REGULATED_COIN_VARIANT,
        };
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
public fun treasury_cap_id<T>(coin_data: &CoinData<T>): Option<ID> {
    coin_data.treasury_cap_id
}

/// Get the deny cap ID for this coin type, if it's a regulated coin.
public fun deny_cap_id<T>(coin_data: &CoinData<T>): Option<ID> {
    match (coin_data.regulated) {
        RegulatedState::Regulated { cap, .. } => option::some(cap),
        RegulatedState::Unregulated => option::none(),
        RegulatedState::Unknown => option::none(),
    }
}

public fun is_frozen<T>(coin_data: &CoinData<T>): bool {
    match (coin_data.supply.borrow()) {
        SupplyState::Frozen(_) => true,
        _ => false,
    }
}

public fun is_deflationary<T>(coin_data: &CoinData<T>): bool {
    match (coin_data.supply.borrow()) {
        SupplyState::Deflationary(_) => true,
        _ => false,
    }
}

/// Check if coin data exists for the given type T in the registry.
public fun exists<T>(registry: &CoinRegistry): bool {
    // TODO: `use derived_object::exists()`
    abort
}

/// Get immutable reference to the coin data from InitCoinData.
public fun inner<T>(init: &InitCoinData<T>): &CoinData<T> {
    &init.data
}

/// Return the ID of the system coin registry object located at address 0xc.
public fun coin_registry_id(): ID {
    @0xc.to_id()
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
public fun create_coin_data_registry_for_testing(ctx: &mut TxContext): CoinRegistry {
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
