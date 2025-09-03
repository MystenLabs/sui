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
use sui::bag::{Self, Bag};
use sui::balance::{Supply, Balance};
use sui::coin::{Self, TreasuryCap, DenyCapV2, CoinMetadata, RegulatedCoinMetadata, Coin};
use sui::derived_object;
use sui::transfer::Receiving;
use sui::vec_map::{Self, VecMap};

#[allow(unused_const)]
/// No Currency found for this coin type.
const ECurrencyNotFound: u64 = 0;
/// Metadata cap already claimed
const EMetadataCapAlreadyClaimed: u64 = 1;
/// Only the system address can create the registry
const ENotSystemAddress: u64 = 2;
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
/// Attempt to migrate legacy metadata for a `Currency` that already exists.
const ECurrencyAlreadyRegistered: u64 = 9;
/// Fixing or making deflationary an empty Supply.
const EEmptySupply: u64 = 10;
/// Attempt to burn a `Currency` that is not deflationary.
const ESupplyNotDeflationary: u64 = 11;

/// Incremental identifier for regulated coin versions in the deny list.
/// 0 here matches DenyCapV2 world.
/// TODO: Fix wording here.
const REGULATED_COIN_VARIANT: u8 = 0;

/// System object found at address 0xc that stores coin data for all
/// registered coin types. This is a shared object that acts as a central
/// registry for coin metadata, supply information, and regulatory status.
public struct CoinRegistry has key {
    id: UID,
}

/// Store only object that enables more flexible coin data
/// registration, allowing for additional fields to be added
/// without changing the Currency structure.
#[allow(unused_field)]
public struct ExtraField(TypeName, vector<u8>) has store;

/// Key used to derive addresses when creating `Currency<T>` objects.
public struct CurrencyKey<phantom T>() has copy, drop, store;

/// Capability object that gates metadata (name, description, icon_url, symbol)
/// changes in the `Currency`. It can only be created (or claimed) once, and can
/// be deleted to prevent changes to the `Currency` metacurrency.
public struct MetadataCap<phantom T> has key, store { id: UID }

// Currency stores metadata such as name, symbol, decimals, icon_url and description,
// as well as supply states (optional) and regulatory status.
public struct Currency<phantom T> has key {
    id: UID,
    /// Number of decimal places the coin uses for display purposes.
    decimals: u8,
    /// Human-readable name for the token.
    name: String,
    /// Short symbol/ticker for the token.
    symbol: String,
    /// Detailed description of the token.
    description: String,
    /// URL for the token's icon/logo.
    icon_url: String,
    /// Current supply state of the coin (fixed supply or unknown)
    /// Note: We're using `Option` because `SupplyState` does not have drop,
    /// meaning we cannot swap out its value at a later state.
    supply: Option<SupplyState<T>>,
    /// Regulatory status of the coin (regulated with deny cap or unknown)
    regulated: RegulatedState,
    /// ID of the treasury cap for this coin type, if registered.
    treasury_cap_id: Option<ID>,
    /// ID of the metadata capability for this coin type, if claimed.
    metadata_cap_id: MetadataCapState,
    /// Additional fields for extensibility.
    extra_fields: VecMap<String, ExtraField>,
}

/// Supply state marks the type of Currency Supply, which can be
/// - Fixed: no minting or burning;
/// - Deflationary: only burning;
/// - Unknown: flexible (supply is controlled by its `TreasuryCap`);
public enum SupplyState<phantom T> has store {
    /// Coin has a fixed supply with the given Supply object.
    Fixed(Supply<T>),
    /// Coin has a supply that can ONLY decrease.
    Deflationary(Supply<T>),
    /// Supply information is not yet known or registered.
    Unknown,
}

/// Regulated state of a coin type.
/// - Regulated: `DenyCap` exists or a `RegulatedCoinMetadata` used to mark currency as regulated;
/// - Unregulated: the currency was created without deny list;
/// - Unknown: the regulatory status is unknown.
public enum RegulatedState has copy, drop, store {
    /// Coin is regulated with a deny cap for address restrictions.
    Regulated { cap: ID, allow_global_pause: bool, variant: u8 },
    /// The coin has been created without deny list.
    Unregulated,
    /// Coin is not regulated or regulatory status is unknown.
    /// Result of a legacy migration for that coin (from `coin.move` constructors)
    Unknown,
}

/// State of the `MetadataCap` for a single `Currency`.
public enum MetadataCapState has copy, drop, store {
    /// The metadata cap has been claimed.
    Claimed(ID),
    /// The metadata cap has not been claimed.
    Unclaimed,
    /// The metadata cap has been claimed and then deleted.
    Deleted,
}

/// Hot potato wrapper to enforce registration after "new_currency" data creation.
/// Destroyed in the `finalize` call and either transferred to the `CoinRegistry`
/// (in case of an OTW registration) or shared directly (for dynamically created
/// currencies).
public struct CurrencyInitializer<phantom T> {
    currency: Currency<T>,
    extra_fields: Bag,
    is_otw: bool,
}

/// Creates a new currency.
///
/// Note: This constructor has no long term difference from `new_currency_with_otw`.
/// The only change is that the first requires an OTW (one-time witness), while
/// this one can be called dynamically from the module that defines `T`, enabling
/// the creation of a new coin type.
public fun new_currency<T: /* internal */ key>(
    registry: &mut CoinRegistry,
    decimals: u8,
    symbol: String,
    name: String,
    description: String,
    icon_url: String,
    ctx: &mut TxContext,
): (CurrencyInitializer<T>, TreasuryCap<T>) {
    assert!(!registry.exists<T>(), ECurrencyAlreadyExists);

    let treasury_cap = coin::new_treasury_cap(ctx);
    let currency = Currency<T> {
        id: derived_object::claim(&mut registry.id, CurrencyKey<T>()),
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

    (CurrencyInitializer { currency, is_otw: false, extra_fields: bag::new(ctx) }, treasury_cap)
}

/// Creates a new currency with using an OTW as proof of uniqueness.
///
/// This is a two-step operation:
/// 1. `Currency` is constructed in the `init` function and sent to the `CoinRegistry`;
/// 2. `Currency` is promoted to a shared object in the `finalize_registration` call;
public fun new_currency_with_otw<T: drop>(
    otw: T,
    decimals: u8,
    symbol: String,
    name: String,
    description: String,
    icon_url: String,
    ctx: &mut TxContext,
): (CurrencyInitializer<T>, TreasuryCap<T>) {
    assert!(sui::types::is_one_time_witness(&otw));
    assert!(is_ascii_printable!(&symbol), EInvalidSymbol);

    let treasury_cap = coin::new_treasury_cap(ctx);
    let currency = Currency<T> {
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

    (CurrencyInitializer { currency, is_otw: true, extra_fields: bag::new(ctx) }, treasury_cap)
}

/// Claim a `MetadataCap` for a coin type.
/// Only allowed from the owner of `TreasuryCap`, and only once.
///
/// Aborts if the `MetadataCap` has already been claimed.
/// Deleted `MetadataCap` cannot be reclaimed.
public fun claim_metadata_cap<T>(
    currency: &mut Currency<T>,
    _: &TreasuryCap<T>,
    ctx: &mut TxContext,
): MetadataCap<T> {
    assert!(!currency.is_metadata_cap_claimed(), EMetadataCapAlreadyClaimed);
    let id = object::new(ctx);
    currency.metadata_cap_id = MetadataCapState::Claimed(id.to_inner());

    MetadataCap { id }
}

// === Currency Initialization ===

/// Allows converting a currency, on init, to regulated, which creates
/// a `DenyCapV2` object, and a denylist entry. Sets regulated state to
/// `Regulated`.
///
/// This action is irreversible.
public fun make_regulated<T>(
    init: &mut CurrencyInitializer<T>,
    allow_global_pause: bool,
    ctx: &mut TxContext,
): DenyCapV2<T> {
    assert!(init.currency.regulated == RegulatedState::Unregulated, EDenyCapAlreadyCreated);
    let deny_cap = coin::new_deny_cap_v2<T>(allow_global_pause, ctx);
    init.currency.regulated =
        RegulatedState::Regulated {
            cap: object::id(&deny_cap),
            allow_global_pause,
            variant: REGULATED_COIN_VARIANT,
        };

    deny_cap
}

use fun make_supply_fixed_init as CurrencyInitializer.make_supply_fixed;

/// Initializer function to make the supply fixed.
/// Aborts if Supply is `0` to enforce minting during initialization.
public fun make_supply_fixed_init<T>(init: &mut CurrencyInitializer<T>, cap: TreasuryCap<T>) {
    assert!(cap.total_supply() > 0, EEmptySupply);
    init.currency.make_supply_fixed(cap)
}

use fun make_supply_deflationary_init as CurrencyInitializer.make_supply_deflationary;

/// Initializer function to make the supply deflationary.
/// Aborts if Supply is `0` to enforce minting during initialization.
public fun make_supply_deflationary_init<T>(
    init: &mut CurrencyInitializer<T>,
    cap: TreasuryCap<T>,
) {
    assert!(cap.total_supply() > 0, EEmptySupply);
    init.currency.make_supply_deflationary(cap)
}

/// Freeze the supply by destroying the `TreasuryCap` and storing it in the `Currency`.
public fun make_supply_fixed<T>(currency: &mut Currency<T>, cap: TreasuryCap<T>) {
    match (currency.supply.swap(SupplyState::Fixed(cap.into_supply()))) {
        // Impossible: We cannot fix a supply or make a supply deflationary twice.
        SupplyState::Fixed(_supply) | SupplyState::Deflationary(_supply) => abort,
        // We replaced "unknown" with fixed supply.
        SupplyState::Unknown => (),
    };
}

/// Make the supply "deflationary" by giving up the `TreasuryCap`, and allowing
/// burning of Coins through the `Currency`.
public fun make_supply_deflationary<T>(currency: &mut Currency<T>, cap: TreasuryCap<T>) {
    match (currency.supply.swap(SupplyState::Deflationary(cap.into_supply()))) {
        // Impossible: We cannot fix a supply or make a supply deflationary twice.
        SupplyState::Fixed(_supply) | SupplyState::Deflationary(_supply) => abort,
        // We replaced "unknown" with frozen supply.
        SupplyState::Unknown => (),
    };
}

#[allow(lint(share_owned))]
public fun finalize<T>(builder: CurrencyInitializer<T>, ctx: &mut TxContext): MetadataCap<T> {
    let CurrencyInitializer { mut currency, is_otw, extra_fields } = builder;
    extra_fields.destroy_empty();
    let id = object::new(ctx);
    currency.metadata_cap_id = MetadataCapState::Claimed(id.to_inner());

    if (is_otw) transfer::transfer(currency, coin_registry_id().to_address())
    else transfer::share_object(currency);

    MetadataCap<T> { id }
}

/// The second step in the "otw" initialization of coin metadata, that takes in
/// the `Currency<T>` that was transferred from init, and transforms it in to a
/// "derived address" shared object.
///
/// Can be performed by anyone.
public fun finalize_registration<T>(
    registry: &mut CoinRegistry,
    currency: Receiving<Currency<T>>,
    _ctx: &mut TxContext,
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
    } = transfer::receive(&mut registry.id, currency);
    id.delete();
    // Now, create the derived version of the coin currency.
    transfer::share_object(Currency {
        id: derived_object::claim(&mut registry.id, CurrencyKey<T>()),
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
public fun delete_metadata_cap<T>(currency: &mut Currency<T>, cap: MetadataCap<T>) {
    let MetadataCap { id } = cap;
    currency.metadata_cap_id = MetadataCapState::Deleted;
    id.delete();
}

/// Allows burning coins for deflationary
public fun burn<T>(currency: &mut Currency<T>, coin: Coin<T>) {
    currency.burn_balance(coin.into_balance());
}

/// Lower level function to burn a `Balance` of a deflationary `Currency`.
public fun burn_balance<T>(currency: &mut Currency<T>, balance: Balance<T>) {
    assert!(currency.is_supply_deflationary(), ESupplyNotDeflationary);
    match (currency.supply.borrow_mut()) {
        SupplyState::Deflationary(supply) => { supply.decrease_supply(balance); },
        _ => abort,
    }
}

// === Currency Setters  ===

/// Enables a metadata cap holder to update a coin's name.
public fun set_name<T>(currency: &mut Currency<T>, _: &MetadataCap<T>, name: String) {
    currency.name = name;
}

/// Enables a metadata cap holder to update a coin's symbol.
public fun set_symbol<T>(currency: &mut Currency<T>, _: &MetadataCap<T>, symbol: String) {
    assert!(is_ascii_printable!(&symbol), EInvalidSymbol);
    currency.symbol = symbol;
}

/// Enables a metadata cap holder to update a coin's description.
public fun set_description<T>(currency: &mut Currency<T>, _: &MetadataCap<T>, description: String) {
    currency.description = description;
}

/// Enables a metadata cap holder to update a coin's icon URL.
public fun set_icon_url<T>(currency: &mut Currency<T>, _: &MetadataCap<T>, icon_url: String) {
    currency.icon_url = icon_url;
}

/// Register the treasury cap ID for a coin type at a later point.
public fun set_treasury_cap_id<T>(currency: &mut Currency<T>, cap: &TreasuryCap<T>) {
    currency.treasury_cap_id.fill(object::id(cap));
}

// == Migrations from legacy coin flows ==

/// Register `CoinMetadata` in the `Registry`. This can happen only once, if the
/// `Currency` did not exist yet. Further updates are possible through
/// `update_from_legacy_metadata`.
public fun migrate_legacy_metadata<T>(
    registry: &mut CoinRegistry,
    legacy: &CoinMetadata<T>,
    ctx: &mut TxContext,
) {
    assert!(!registry.exists<T>(), ECurrencyAlreadyRegistered);
    transfer::share_object(Currency<T> {
        id: derived_object::claim(&mut registry.id, CurrencyKey<T>()),
        decimals: legacy.get_decimals(),
        name: legacy.get_name(),
        symbol: legacy.get_symbol().to_string(),
        description: legacy.get_description(),
        icon_url: legacy
            .get_icon_url()
            .map!(|url| url.inner_url().to_string())
            .destroy_or!(b"".to_string()),
        supply: option::some(SupplyState::Unknown),
        regulated: RegulatedState::Unknown, // We don't know if it's regulated or not!
        treasury_cap_id: option::none(),
        metadata_cap_id: MetadataCapState::Unclaimed,
        extra_fields: vec_map::empty(),
    });
}

/// Update `Currency` from `CoinMetadata` as long as the `MetadataCap` is not claimed.
public fun update_from_legacy_metadata<T>(currency: &mut Currency<T>, legacy: &CoinMetadata<T>) {
    assert!(!currency.is_metadata_cap_claimed(), ECannotUpdateManagedMetadata);
    currency.name = legacy.get_name();
    currency.symbol = legacy.get_symbol().to_string();
    currency.description = legacy.get_description();
    currency.decimals = legacy.get_decimals();
    currency.icon_url =
        legacy.get_icon_url().map!(|url| url.inner_url().to_string()).destroy_or!(b"".to_string());
}

/// Delete the legacy `CoinMetadata` object if the metadata cap for the new registry
/// has already been claimed.
///
/// This function is only callable after there's "proof" that the author of the coin
/// can manage the metadata using the registry system (so having a metadata cap claimed).
public fun delete_migrated_legacy_metadata<T>(currency: &mut Currency<T>, legacy: CoinMetadata<T>) {
    assert!(currency.is_metadata_cap_claimed(), EMetadataCapNotClaimed);
    legacy.destroy_metadata();
}

/// Allow migrating the regulated state by access to `RegulatedCoinMetadata` frozen object.
/// This is a permissionless operation which can be performed only once.
public fun migrate_regulated_state_by_metadata<T>(
    currency: &mut Currency<T>,
    metadata: &RegulatedCoinMetadata<T>,
) {
    // Only allow if this hasn't been migrated before.
    assert!(currency.regulated == RegulatedState::Unknown, EDenyListStateAlreadySet);
    currency.regulated =
        RegulatedState::Regulated {
            cap: metadata.deny_cap_id(),
            variant: REGULATED_COIN_VARIANT,
        };
}

/// Allow migrating the regulated state by a `DenyCapV2` object.
public fun migrate_regulated_state_by_cap<T>(currency: &mut Currency<T>, cap: &DenyCapV2<T>) {
    currency.regulated =
        RegulatedState::Regulated {
            cap: object::id(cap),
            variant: REGULATED_COIN_VARIANT,
        };
}

// === Public getters  ===

/// Get the number of decimal places for the coin type.
public fun decimals<T>(currency: &Currency<T>): u8 { currency.decimals }

/// Get the human-readable name of the coin.
public fun name<T>(currency: &Currency<T>): String { currency.name }

/// Get the symbol/ticker of the coin.
public fun symbol<T>(currency: &Currency<T>): String { currency.symbol }

/// Get the description of the coin.
public fun description<T>(currency: &Currency<T>): String {
    currency.description
}

/// Get the icon URL for the coin.
public fun icon_url<T>(currency: &Currency<T>): String { currency.icon_url }

/// Check if the metadata capability has been claimed for this coin type.
public fun is_metadata_cap_claimed<T>(currency: &Currency<T>): bool {
    match (currency.metadata_cap_id) {
        MetadataCapState::Claimed(_) | MetadataCapState::Deleted => true,
        _ => false,
    }
}

public fun metadata_cap_id<T>(currency: &Currency<T>): Option<ID> {
    match (currency.metadata_cap_id) {
        MetadataCapState::Claimed(id) => option::some(id),
        _ => option::none(),
    }
}

/// Get the treasury cap ID for this coin type, if registered.
public fun treasury_cap_id<T>(currency: &Currency<T>): Option<ID> {
    currency.treasury_cap_id
}

/// Get the deny cap ID for this coin type, if it's a regulated coin.
public fun deny_cap_id<T>(currency: &Currency<T>): Option<ID> {
    match (currency.regulated) {
        RegulatedState::Regulated { cap, .. } => option::some(cap),
        RegulatedState::Unregulated => option::none(),
        RegulatedState::Unknown => option::none(),
    }
}

public fun is_supply_fixed<T>(currency: &Currency<T>): bool {
    match (currency.supply.borrow()) {
        SupplyState::Fixed(_) => true,
        _ => false,
    }
}

public fun is_supply_deflationary<T>(currency: &Currency<T>): bool {
    match (currency.supply.borrow()) {
        SupplyState::Deflationary(_) => true,
        _ => false,
    }
}

public fun is_regulated<T>(currency: &Currency<T>): bool {
    match (currency.regulated) {
        RegulatedState::Regulated { .. } => true,
        _ => false,
    }
}

/// Get the total supply for the `Currency<T>` if the Supply is in fixed or
/// deflationary state. Returns `None` if the supply is unknown.
public fun total_supply<T>(currency: &Currency<T>): Option<u64> {
    match (currency.supply.borrow()) {
        SupplyState::Fixed(supply) => option::some(supply.value()),
        SupplyState::Deflationary(supply) => option::some(supply.value()),
        SupplyState::Unknown => option::none(),
    }
}

/// Check if coin data exists for the given type T in the registry.
public fun exists<T>(registry: &CoinRegistry): bool {
    derived_object::exists(&registry.id, CurrencyKey<T>())
}

/// Return the ID of the system coin registry object located at address 0xc.
public fun coin_registry_id(): ID {
    @0xc.to_id()
}

#[allow(unused_function)]
/// Create and share the singleton Registry -- this function is
/// called exactly once, during the upgrade epoch.
/// Only the system address (0x0) can create the registry.
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
/// Unwrap CurrencyInitializer for testing purposes.
/// This function is test-only and should only be used in tests.
public fun unwrap_for_testing<T>(init: CurrencyInitializer<T>): Currency<T> {
    let CurrencyInitializer { currency, extra_fields, .. } = init;
    extra_fields.destroy_empty();
    currency
}

#[test_only]
public fun finalize_unwrap_for_testing<T>(
    init: CurrencyInitializer<T>,
    ctx: &mut TxContext,
): (Currency<T>, MetadataCap<T>) {
    let CurrencyInitializer { mut currency, extra_fields, .. } = init;
    extra_fields.destroy_empty();
    let id = object::new(ctx);
    currency.metadata_cap_id = MetadataCapState::Claimed(id.to_inner());
    (currency, MetadataCap { id })
}

#[test_only]
public fun migrate_legacy_metadata_for_testing<T>(
    registry: &mut CoinRegistry,
    legacy: &CoinMetadata<T>,
    _ctx: &mut TxContext,
): Currency<T> {
    assert!(!registry.exists<T>(), ECurrencyAlreadyRegistered);

    Currency<T> {
        id: derived_object::claim(&mut registry.id, CurrencyKey<T>()),
        decimals: legacy.get_decimals(),
        name: legacy.get_name(),
        symbol: legacy.get_symbol().to_string(),
        description: legacy.get_description(),
        icon_url: legacy
            .get_icon_url()
            .map!(|url| url.inner_url().to_string())
            .destroy_or!(b"".to_string()),
        supply: option::some(SupplyState::Unknown),
        regulated: RegulatedState::Unknown,
        treasury_cap_id: option::none(),
        metadata_cap_id: MetadataCapState::Unclaimed,
        extra_fields: vec_map::empty(),
    }
}
