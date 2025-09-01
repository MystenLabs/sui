// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

/// Defines the system object for managing coin data in a central
/// registry. This module provides a centralized way to store and manage
/// metadata for all coin types in the Sui ecosystem, including their
/// supply information, regulatory status, and metadata capabilities.
module sui::coin_registry;

use std::ascii;
use std::string::String;
use std::type_name::TypeName;
use sui::balance::Supply;
use sui::coin::{Self, TreasuryCap, DenyCapV2, CoinMetadata, RegulatedCoinMetadata, Coin};
use sui::transfer::Receiving;
use sui::vec_map::{Self, VecMap};

#[allow(unused_const)]
/// No Currency found for this coin type.
const ECurrencyNotFound: u64 = 0;
/// Metadata cap already claimed
const EMetadataCapAlreadyClaimed: u64 = 1;
/// Only the system address can create the registry
const ENotSystemAddress: u64 = 2;
#[allow(unused_const)]
/// Currency for this coin type already exists
const ECurrencyAlreadyExists: u64 = 3;
/// Attempt to set the deny list state permissionlessly while it has already been set.
const EDenyListStateAlreadySet: u64 = 4;
/// Tries to delete legacy metadata without having claimed the management capability.
const EMetadataCapNotClaimed: u64 = 5;
/// Attempt to update `Currency` with legacy metadata after the `MetadataCap` has
/// been claimed. Updates are only allowed if the `MetadataCap` has not yet been
/// claimed or deleted.
const ECannotUpdateManagedMetadata: u64 = 6;
/// Attempt to set the symbol to a non-ASCII printable character
const EInvalidSymbol: u64 = 7;
/// Attempt to set the deny cap twice.
const EDenyCapAlreadyCreated: u64 = 8;

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
/// without changing the Currency structure.
#[allow(unused_field)]
public struct ExtraField(TypeName, vector<u8>) has store;

/// Key used to access coin metadata hung off the `CoinRegistry`
/// object. This key can be versioned to allow for future changes
/// to the metadata object.
public struct CurrencyKey<phantom T>() has copy, drop, store;

/// Capability object that gates metadata (name, description, icon_url, symbol)
/// changes in the `Currency`. It can only be created (or claimed) once, and can
/// be deleted to prevent changes to the `Currency` metadata.
public struct MetadataCap<phantom T> has key, store { id: UID }

/// Currency object that stores comprehensive information about a coin type.
/// This includes metadata like name, symbol, and description, as well as
/// supply and regulatory status information.
public struct Currency<phantom T> has key {
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
    metadata_cap_id: MetadataCapState,
    /// Additional fields for extensibility
    extra_fields: VecMap<String, ExtraField>,
}

/// Supply state of a coin type, which can be fixed (with a known supply)
/// or unknown (supply not yet registered in the registry).
public enum SupplyState<phantom T> has store {
    /// Coin has a fixed supply with the given Supply object
    Fixed(Supply<T>),
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

/// State of the `MetadataCap` for a single `Currency`.
public enum MetadataCapState has copy, drop, store {
    /// The metadata cap has been claimed.
    Claimed(ID),
    /// The metadata cap has not been claimed.
    Unclaimed,
    /// The metadata cap has been deleted (so the `Currency` metadata cannot be updated).
    Deleted,
}

/// Hot potato wrapper to enforce registration after "create_currency" data creation.
/// Destroyed in the `finalize` call and either transferred to the `CoinRegistry`
/// (in case of an OTW registration) or shared directly (for dynamically created
/// currencies).
public struct CurrencyBuilder<phantom T> {
    data: Currency<T>,
    is_otw: bool,
}

// 1. Entrypoint for creating currency [done]
// 2. Entrypoint for creating regulated currency [done]
// 3. Claim capability (using treasury cap) [done]
// 3. Migrate existing CoinMetadada -> Registry Metadata
public fun new_currency<T: drop>(
    otw: T,
    decimals: u8,
    symbol: String,
    name: String,
    description: String,
    icon_url: String,
    ctx: &mut TxContext,
): (CurrencyBuilder<T>, TreasuryCap<T>) {
    // Make sure there's only one instance of the type T, using an OTW check.
    assert!(sui::types::is_one_time_witness(&otw));
    // Hacky check to make sure the Symbol is ASCII.
    assert!(is_ascii_printable!(&symbol), EInvalidSymbol);

    let treasury_cap = coin::new_treasury_cap(ctx);

    let metadata = Currency<T> {
        id: object::new(ctx),
        decimals,
        name,
        symbol,
        description,
        icon_url,
        supply: option::some(SupplyState::Unknown),
        regulated: RegulatedState::Unregulated,
        treasury_cap_id: option::some(object::id(&treasury_cap)),
        metadata_cap_id: MetadataCapState::Unclaimed,
        extra_fields: vec_map::empty(),
    };

    (CurrencyBuilder { data: metadata, is_otw: true }, treasury_cap)
}

/// Create a currency dynamically.
/// TODO: Add verifier rule, as this needs to only be callable by the module that defines `T`.
public fun new_dynamic_currency<T: /* internal */ key>(
    registry: &mut CoinRegistry,
    decimals: u8,
    symbol: String,
    name: String,
    description: String,
    icon_url: String,
    ctx: &mut TxContext,
): (CurrencyBuilder<T>, TreasuryCap<T>) {
    // Unlike OTW creation, the guarantee on not having duplicate currencies come from the
    // coin registry.
    assert!(!registry.exists<T>());

    let treasury_cap = coin::new_treasury_cap(ctx);

    let metadata = Currency<T> {
        // TODO: use `derived_object::claim(&mut registry.id, CoinKey<T>())`
        id: object::new(ctx),
        decimals,
        name,
        symbol,
        description,
        icon_url,
        supply: option::some(SupplyState::Unknown),
        regulated: RegulatedState::Unregulated,
        treasury_cap_id: option::some(object::id(&treasury_cap)),
        metadata_cap_id: MetadataCapState::Unclaimed,
        extra_fields: vec_map::empty(),
    };

    (CurrencyBuilder { data: metadata, is_otw: false }, treasury_cap)
}

/// Claim a MetadataCap for a coin type. This is only allowed from the owner of
/// `TreasuryCap`, and only once.
///
/// Aborts if the metadata capability has already been claimed.
/// If `MetadataCap` was deleted, it cannot be claimed!
public fun claim_cap<T>(
    data: &mut Currency<T>,
    _: &TreasuryCap<T>,
    ctx: &mut TxContext,
): MetadataCap<T> {
    assert!(!data.is_metadata_cap_claimed(), EMetadataCapAlreadyClaimed);
    let id = object::new(ctx);
    data.metadata_cap_id = MetadataCapState::Claimed(id.to_inner());

    MetadataCap { id }
}

/// Allows converting a currency, on init, to regulated, which creates
/// a `DenyCapV2` object, and a denylist entry.
///
/// This is only possible when initializing a coin (cannot be done for existing coins).
public fun make_regulated<T>(
    init: &mut CurrencyBuilder<T>,
    allow_global_pause: bool,
    ctx: &mut TxContext,
): DenyCapV2<T> {
    assert!(init.data.regulated == RegulatedState::Unregulated, EDenyCapAlreadyCreated);
    let deny_cap = coin::new_deny_cap_v2<T>(allow_global_pause, ctx);

    init.inner_mut().regulated =
        RegulatedState::Regulated {
            cap: object::id(&deny_cap),
            variant: REGULATED_COIN_VARIANT,
        };

    deny_cap
}

/// Freeze the supply by destroying the TreasuryCap and storing it in the Currency.
public fun make_supply_fixed<T>(data: &mut Currency<T>, cap: TreasuryCap<T>) {
    match (data.supply.swap(SupplyState::Fixed(cap.into_supply()))) {
        // Impossible: We cannot fix a supply or make a supply deflationary twice.
        SupplyState::Fixed(_supply) | SupplyState::Deflationary(_supply) => abort,
        // We replaced "unknown" with fixed supply.
        SupplyState::Unknown => (),
    };
}

/// Make the supply "deflatinary" by destroying the TreasuryCap and taking control of the
/// supply through the Currency.
public fun make_supply_deflationary<T>(data: &mut Currency<T>, cap: TreasuryCap<T>) {
    match (data.supply.swap(SupplyState::Deflationary(cap.into_supply()))) {
        // Impossible: We cannot fix a supply or make a supply deflationary twice.
        SupplyState::Fixed(_supply) | SupplyState::Deflationary(_supply) => abort,
        // We replaced "unknown" with frozen supply.
        SupplyState::Unknown => (),
    };
}

#[allow(lint(share_owned))]
public fun finalize<T>(builder: CurrencyBuilder<T>, ctx: &mut TxContext): MetadataCap<T> {
    let CurrencyBuilder { mut data, is_otw } = builder;

    let id = object::new(ctx);
    data.metadata_cap_id = MetadataCapState::Claimed(id.to_inner());

    if (is_otw) transfer::transfer(data, coin_registry_id().to_address())
    else transfer::share_object(data);

    MetadataCap<T> { id }
}

/// The second step in the "otw" initialization of coin metadata, that takes in the `Currency<T>` that was
/// transferred from init, and transforms it in to a "derived address" shared object.
public fun finalize_registration<T>(
    registry: &mut CoinRegistry,
    coin_data: Receiving<Currency<T>>,
    ctx: &mut TxContext,
) {
    // 1. Consume Currency
    // 2. Re-create it with a "derived" address.
    let Currency {
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
    transfer::share_object(Currency {
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

/// Delete the metadata cap making further updates of `Currency` metadata impossible.
/// This action is IRREVERSIBLE, and the `MetadataCap` can no longer be claimed.
public fun delete_metadata_cap<T>(data: &mut Currency<T>, cap: MetadataCap<T>) {
    let MetadataCap { id } = cap;
    data.metadata_cap_id = MetadataCapState::Deleted;
    id.delete();
}

/// Get mutable reference to the coin data from CurrencyBuilder.
/// This function is package-private and should only be called by the coin module.
public fun inner_mut<T>(init: &mut CurrencyBuilder<T>): &mut Currency<T> {
    &mut init.data
}

/// Allows burning coins for deflationary
public fun burn<T>(data: &mut Currency<T>, coin: Coin<T>) {
    assert!(data.is_supply_deflationary());

    match (data.supply.borrow_mut()) {
        SupplyState::Deflationary(supply) => { supply.decrease_supply(coin.into_balance()); },
        _ => abort,
    }
}

// === Currency Setters  ===

/// Enables a metadata cap holder to update a coin's name.
public fun set_name<T>(data: &mut Currency<T>, _: &MetadataCap<T>, name: String) {
    name.to_ascii();
    data.name = name;
}

/// Enables a metadata cap holder to update a coin's symbol.
/// TODO: Should we kill this? :)
public fun set_symbol<T>(data: &mut Currency<T>, _: &MetadataCap<T>, symbol: String) {
    assert!(is_ascii_printable!(&symbol), EInvalidSymbol);
    data.symbol = symbol;
}

/// Enables a metadata cap holder to update a coin's description.
public fun set_description<T>(data: &mut Currency<T>, _: &MetadataCap<T>, description: String) {
    data.description = description;
}

/// Enables a metadata cap holder to update a coin's icon URL.
public fun set_icon_url<T>(data: &mut Currency<T>, _: &MetadataCap<T>, icon_url: String) {
    data.icon_url = icon_url;
}

/// Register the treasury cap ID for a coin type at a later point.
public fun set_treasury_cap_id<T>(data: &mut Currency<T>, cap: &TreasuryCap<T>) {
    data.treasury_cap_id.fill(object::id(cap));
}

// == Migrations from legacy coin flows ==

/// TODO: Register legacy coin metadata to the registry --
/// This should:
/// 1. Take the old metadata
/// 2. Create a `Currency<T>` object with a derived address (and share it!)
public fun migrate_legacy_metadata<T>(_registry: &mut CoinRegistry, _v1: &CoinMetadata<T>) {
    abort
}

/// TODO: Allow coin metadata to be updated from legacy as described in our docs.
public fun update_from_legacy_metadata<T>(data: &mut Currency<T>, _v1: &CoinMetadata<T>) {
    assert!(!data.is_metadata_cap_claimed(), ECannotUpdateManagedMetadata);
    abort
}

/// Delete the legacy `CoinMetadata` object if the metadata cap for the new registry
/// has already been claimed.
///
/// This function is only callable after there's "proof" that the author of the coin
/// can manage the metadata using the registry system (so having a metadata cap claimed).
public fun delete_migrated_legacy_metadata<T>(data: &mut Currency<T>, v1: CoinMetadata<T>) {
    assert!(data.is_metadata_cap_claimed(), EMetadataCapNotClaimed);
    v1.destroy_metadata();
}

/// Allow migrating the regulated state by access to `RegulatedCoinMetadata` frozen object.
/// This is a permissionless operation.
public fun migrate_regulated_state_by_metadata<T>(
    data: &mut Currency<T>,
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
public fun migrate_regulated_state_by_cap<T>(data: &mut Currency<T>, cap: &DenyCapV2<T>) {
    data.regulated =
        RegulatedState::Regulated {
            cap: object::id(cap),
            variant: REGULATED_COIN_VARIANT,
        };
}

// === Public getters  ===

/// Get the number of decimal places for the coin type.a
public fun decimals<T>(coin_data: &Currency<T>): u8 { coin_data.decimals }

/// Get the human-readable name of the coin.
public fun name<T>(coin_data: &Currency<T>): String { coin_data.name }

/// Get the symbol/ticker of the coin.
public fun symbol<T>(coin_data: &Currency<T>): String { coin_data.symbol }

/// Get the description of the coin.
public fun description<T>(coin_data: &Currency<T>): String {
    coin_data.description
}

/// Get the icon URL for the coin.
public fun icon_url<T>(coin_data: &Currency<T>): String { coin_data.icon_url }

/// Check if the metadata capability has been claimed for this coin type.
public fun is_metadata_cap_claimed<T>(coin_data: &Currency<T>): bool {
    match (coin_data.metadata_cap_id) {
        MetadataCapState::Claimed(_) | MetadataCapState::Deleted => true,
        _ => false,
    }
}

public fun metadata_cap_id<T>(coin_data: &Currency<T>): Option<ID> {
    match (coin_data.metadata_cap_id) {
        MetadataCapState::Claimed(id) => option::some(id),
        _ => option::none(),
    }
}

/// Get the treasury cap ID for this coin type, if registered.
public fun treasury_cap_id<T>(coin_data: &Currency<T>): Option<ID> {
    coin_data.treasury_cap_id
}

/// Get the deny cap ID for this coin type, if it's a regulated coin.
public fun deny_cap_id<T>(coin_data: &Currency<T>): Option<ID> {
    match (coin_data.regulated) {
        RegulatedState::Regulated { cap, .. } => option::some(cap),
        RegulatedState::Unregulated => option::none(),
        RegulatedState::Unknown => option::none(),
    }
}

public fun is_supply_fixed<T>(coin_data: &Currency<T>): bool {
    match (coin_data.supply.borrow()) {
        SupplyState::Fixed(_) => true,
        _ => false,
    }
}

public fun is_supply_deflationary<T>(coin_data: &Currency<T>): bool {
    match (coin_data.supply.borrow()) {
        SupplyState::Deflationary(_) => true,
        _ => false,
    }
}

public fun is_regulated<T>(coin_data: &Currency<T>): bool {
    match (coin_data.regulated) {
        RegulatedState::Regulated { .. } => true,
        _ => false,
    }
}

/// Get the total supply for the `Currency<T>` if the Supply is in fixed or
/// deflationary state. Returns `None` if the supply is unknown.
public fun total_supply<T>(coin_data: &Currency<T>): Option<u64> {
    match (coin_data.supply.borrow()) {
        SupplyState::Fixed(supply) => option::some(supply.value()),
        SupplyState::Deflationary(supply) => option::some(supply.value()),
        SupplyState::Unknown => option::none(),
    }
}

#[allow(unused_type_parameter)]
/// Check if coin data exists for the given type T in the registry.
public fun exists<T>(_registry: &CoinRegistry): bool {
    // TODO: `use derived_object::exists()`
    false // TODO: return function call once derived addresses are in!
}

/// Get immutable reference to the coin data from CurrencyBuilder.
public fun inner<T>(init: &CurrencyBuilder<T>): &Currency<T> {
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
///
/// TODO: use `&TxContext` and use correct id.
fun create(ctx: &mut TxContext) {
    assert!(ctx.sender() == @0x0, ENotSystemAddress);

    transfer::share_object(CoinRegistry {
        // id: object::sui_coin_registry_object_id(),
        id: object::new(ctx),
    });
}

/// Nit: consider adding this function to `std::string` in the future.
macro fun is_ascii_printable($s: &String): bool {
    let s = $s;
    s.as_bytes().all!(|b| ascii::is_printable_char(*b))
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
/// Unwrap CurrencyBuilder for testing purposes.
/// This function is test-only and should only be used in tests.
public fun unwrap_for_testing<T>(init: CurrencyBuilder<T>): Currency<T> {
    let CurrencyBuilder { data, .. } = init;
    data
}

#[test_only]
public fun finalize_unwrap_for_testing<T>(
    init: CurrencyBuilder<T>,
    ctx: &mut TxContext,
): (Currency<T>, MetadataCap<T>) {
    let CurrencyBuilder { mut data, .. } = init;
    let id = object::new(ctx);
    data.metadata_cap_id = MetadataCapState::Claimed(id.to_inner());
    (data, MetadataCap { id })
}
