// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

/// Defines the system object for managing coin data in a central
/// registry. This module provides a centralized way to store and manage
/// metadata for all currencies in the Sui ecosystem, including their
/// supply information, regulatory status, and metadata capabilities.
module sui::coin_registry;

use std::ascii;
use std::string::String;
use std::type_name::{Self, TypeName};
use sui::bag::{Self, Bag};
use sui::balance::{Supply, Balance};
use sui::coin::{Self, TreasuryCap, DenyCapV2, CoinMetadata, RegulatedCoinMetadata, Coin};
use sui::derived_object;
use sui::dynamic_field as df;
use sui::transfer::Receiving;
use sui::vec_map::{Self, VecMap};

/// Metadata cap already claimed
#[error(code = 0)]
const EMetadataCapAlreadyClaimed: vector<u8> = b"Metadata cap already claimed";
/// Only the system address can create the registry
#[error(code = 1)]
const ENotSystemAddress: vector<u8> = b"Only the system can create the registry.";
/// Currency for this coin type already exists
#[error(code = 2)]
const ECurrencyAlreadyExists: vector<u8> = b"Currency for this coin type already exists.";
/// Attempt to set the deny list state permissionlessly while it has already been set.
#[error(code = 3)]
const EDenyListStateAlreadySet: vector<u8> =
    b"Cannot set the deny list state as it has already been set.";
/// Attempt to update `Currency` with legacy metadata after the `MetadataCap` has
/// been claimed. Updates are only allowed if the `MetadataCap` has not yet been
/// claimed or deleted.
#[error(code = 5)]
const ECannotUpdateManagedMetadata: vector<u8> =
    b"Cannot update metadata whose `MetadataCap` has already been claimed";
/// Attempt to set the symbol to a non-ASCII printable character
#[error(code = 6)]
const EInvalidSymbol: vector<u8> = b"Symbol has to be ASCII printable";
#[error(code = 7)]
const EDenyCapAlreadyCreated: vector<u8> = b"Cannot claim the deny cap twice";
/// Attempt to migrate legacy metadata for a `Currency` that already exists.
#[error(code = 8)]
const ECurrencyAlreadyRegistered: vector<u8> = b"Currency already registered";
#[error(code = 9)]
const EEmptySupply: vector<u8> = b"Supply cannot be empty";
#[error(code = 10)]
const ESupplyNotBurnOnly: vector<u8> = b"Cannot burn on a non burn-only supply";
#[error(code = 11)]
const EInvariantViolation: vector<u8> = b"Code invariant violation";
#[error(code = 12)]
const EDeletionNotSupported: vector<u8> = b"Deleting legacy metadata is not supported";
#[error(code = 13)]
const ENotOneTimeWitness: vector<u8> = b"Type is expected to be OTW";
#[error(code = 14)]
const EBorrowLegacyMetadata: vector<u8> = b"Cannot borrow legacy metadata for migrated currency";
#[error(code = 15)]
const EDuplicateBorrow: vector<u8> = b"Attempt to return duplicate borrowed CoinMetadata";

/// Incremental identifier for regulated coin versions in the deny list.
/// We start from `0` in the new system, which aligns with the state of `DenyCapV2`.
const REGULATED_COIN_VERSION: u8 = 0;

/// Marker used in metadata to indicate that the currency is not migrated.
const NEW_CURRENCY_MARKER: vector<u8> = b"is_new_currency";

/// System object found at address `0xc` that stores coin data for all
/// registered coin types. This is a shared object that acts as a central
/// registry for coin metadata, supply information, and regulatory status.
public struct CoinRegistry has key { id: UID }

/// Store only object that enables more flexible coin data
/// registration, allowing for additional fields to be added
/// without changing the `Currency` structure.
public struct ExtraField(TypeName, vector<u8>) has store;

/// Key used to derive addresses when creating `Currency<T>` objects.
public struct CurrencyKey<phantom T>() has copy, drop, store;

/// Key used to store the legacy `CoinMetadata` for a `Currency`.
public struct LegacyMetadataKey() has copy, drop, store;

/// Capability object that gates metadata (name, description, icon_url, symbol)
/// changes in the `Currency`. It can only be created (or claimed) once, and can
/// be deleted to prevent changes to the `Currency` metadata.
public struct MetadataCap<phantom T> has key, store { id: UID }

/// Potato callback for the legacy `CoinMetadata` borrowing.
public struct Borrow<phantom T> {}

/// Currency stores metadata such as name, symbol, decimals, icon_url and description,
/// as well as supply states (optional) and regulatory status.
public struct Currency<phantom T> has key {
    id: UID,
    /// Number of decimal places the coin uses for display purposes.
    decimals: u8,
    /// Human-readable name for the coin.
    name: String,
    /// Short symbol/ticker for the coin.
    symbol: String,
    /// Detailed description of the coin.
    description: String,
    /// URL for the coin's icon/logo.
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
/// - BurnOnly: no minting, burning is allowed;
/// - Unknown: flexible (supply is controlled by its `TreasuryCap`);
public enum SupplyState<phantom T> has store {
    /// Coin has a fixed supply with the given Supply object.
    Fixed(Supply<T>),
    /// Coin has a supply that can ONLY decrease.
    BurnOnly(Supply<T>),
    /// Supply information is not yet known or registered.
    Unknown,
}

/// Regulated state of a coin type.
/// - Regulated: `DenyCap` exists or a `RegulatedCoinMetadata` used to mark currency as regulated;
/// - Unregulated: the currency was created without deny list;
/// - Unknown: the regulatory status is unknown.
public enum RegulatedState has copy, drop, store {
    /// Coin is regulated with a deny cap for address restrictions.
    /// `allow_global_pause` is `None` if the information is unknown (has not been migrated from `DenyCapV2`).
    Regulated { cap: ID, allow_global_pause: Option<bool>, variant: u8 },
    /// The coin has been created without deny list.
    Unregulated,
    /// Regulatory status is unknown.
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
/// This can be called from the module that defines `T` any time after it has been published.
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
    assert!(is_ascii_printable!(&symbol), EInvalidSymbol);

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
    assert!(sui::types::is_one_time_witness(&otw), ENotOneTimeWitness);
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
            allow_global_pause: option::some(allow_global_pause),
            variant: REGULATED_COIN_VERSION,
        };

    deny_cap
}

public use fun make_supply_fixed_init as CurrencyInitializer.make_supply_fixed;

/// Initializer function to make the supply fixed.
/// Aborts if Supply is `0` to enforce minting during initialization.
public fun make_supply_fixed_init<T>(init: &mut CurrencyInitializer<T>, cap: TreasuryCap<T>) {
    assert!(cap.total_supply() > 0, EEmptySupply);
    init.currency.make_supply_fixed(cap)
}

public use fun make_supply_burn_only_init as CurrencyInitializer.make_supply_burn_only;

/// Initializer function to make the supply burn-only.
/// Aborts if Supply is `0` to enforce minting during initialization.
public fun make_supply_burn_only_init<T>(init: &mut CurrencyInitializer<T>, cap: TreasuryCap<T>) {
    assert!(cap.total_supply() > 0, EEmptySupply);
    init.currency.make_supply_burn_only(cap)
}

/// Freeze the supply by destroying the `TreasuryCap` and storing it in the `Currency`.
public fun make_supply_fixed<T>(currency: &mut Currency<T>, cap: TreasuryCap<T>) {
    match (currency.supply.swap(SupplyState::Fixed(cap.into_supply()))) {
        // Impossible: We cannot fix a supply or make a supply burn-only twice.
        SupplyState::Fixed(_supply) | SupplyState::BurnOnly(_supply) => abort EInvariantViolation,
        // We replaced "unknown" with fixed supply.
        SupplyState::Unknown => (),
    };
}

/// Make the supply `BurnOnly` by giving up the `TreasuryCap`, and allowing
/// burning of Coins through the `Currency`.
public fun make_supply_burn_only<T>(currency: &mut Currency<T>, cap: TreasuryCap<T>) {
    match (currency.supply.swap(SupplyState::BurnOnly(cap.into_supply()))) {
        // Impossible: We cannot fix a supply or make a supply burn-only twice.
        SupplyState::Fixed(_supply) | SupplyState::BurnOnly(_supply) => abort EInvariantViolation,
        // We replaced "unknown" with frozen supply.
        SupplyState::Unknown => (),
    };
}

#[allow(lint(share_owned))]
/// Finalize the coin initialization, returning `MetadataCap`
public fun finalize<T>(builder: CurrencyInitializer<T>, ctx: &mut TxContext): MetadataCap<T> {
    let is_otw = builder.is_otw;
    let (currency, metadata_cap) = finalize_impl!(builder, ctx);

    // Either share directly (`new_currency` scenario), or transfer as TTO to `CoinRegistry`.
    if (is_otw) transfer::transfer(currency, object::sui_coin_registry_address())
    else transfer::share_object(currency);

    metadata_cap
}

#[allow(lint(share_owned))]
/// Does the same as `finalize`, but also deletes the `MetadataCap` after finalization.
public fun finalize_and_delete_metadata_cap<T>(
    builder: CurrencyInitializer<T>,
    ctx: &mut TxContext,
) {
    let is_otw = builder.is_otw;
    let (mut currency, metadata_cap) = finalize_impl!(builder, ctx);

    currency.delete_metadata_cap(metadata_cap);

    // Either share directly (`new_currency` scenario), or transfer as TTO to `CoinRegistry`.
    if (is_otw) transfer::transfer(currency, object::sui_coin_registry_address())
    else transfer::share_object(currency);
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

/// Burn the `Coin` if the `Currency` has a `BurnOnly` supply state.
public fun burn<T>(currency: &mut Currency<T>, coin: Coin<T>) {
    currency.burn_balance(coin.into_balance());
}

/// Burn the `Balance` if the `Currency` has a `BurnOnly` supply state.
public fun burn_balance<T>(currency: &mut Currency<T>, balance: Balance<T>) {
    assert!(currency.is_supply_burn_only(), ESupplyNotBurnOnly);
    match (currency.supply.borrow_mut()) {
        SupplyState::BurnOnly(supply) => { supply.decrease_supply(balance); },
        _ => abort EInvariantViolation, // unreachable
    }
}

// === Currency Setters  ===

/// Update the name of the `Currency`.
public fun set_name<T>(currency: &mut Currency<T>, _: &MetadataCap<T>, name: String) {
    currency.name = name;
}

/// Update the description of the `Currency`.
public fun set_description<T>(currency: &mut Currency<T>, _: &MetadataCap<T>, description: String) {
    currency.description = description;
}

/// Update the icon URL of the `Currency`.
public fun set_icon_url<T>(currency: &mut Currency<T>, _: &MetadataCap<T>, icon_url: String) {
    currency.icon_url = icon_url;
}

/// Register the treasury cap ID for a migrated `Currency`. All currencies created with
/// `new_currency` or `new_currency_with_otw` have their treasury cap ID set during
/// initialization.
public fun set_treasury_cap_id<T>(currency: &mut Currency<T>, cap: &TreasuryCap<T>) {
    currency.treasury_cap_id.fill(object::id(cap));
}

// == Migrations from legacy coin flows ==

/// Register `CoinMetadata` in the `CoinRegistry`. This can happen only once, if the
/// `Currency` did not exist yet. Further updates are possible through
/// `update_from_legacy_metadata`.
public fun migrate_legacy_metadata<T>(
    registry: &mut CoinRegistry,
    legacy: &CoinMetadata<T>,
    _ctx: &mut TxContext,
) {
    let currency = migrate_legacy_metadata_impl!(registry, legacy);
    transfer::share_object(currency);
}

/// Update `Currency` from `CoinMetadata` if the `MetadataCap` is not claimed. After
/// the `MetadataCap` is claimed, updates can only be made through `set_*` functions.
public fun update_from_legacy_metadata<T>(currency: &mut Currency<T>, legacy: &CoinMetadata<T>) {
    assert!(!currency.is_metadata_cap_claimed(), ECannotUpdateManagedMetadata);

    currency.name = legacy.get_name();
    currency.symbol = legacy.get_symbol().to_string();
    currency.description = legacy.get_description();
    currency.decimals = legacy.get_decimals();
    currency.icon_url =
        legacy.get_icon_url().map!(|url| url.inner_url().to_string()).destroy_or!(b"".to_string());
}

#[deprecated(note = b"Method disabled")]
public fun delete_migrated_legacy_metadata<T>(_: &mut Currency<T>, _: CoinMetadata<T>) {
    abort EDeletionNotSupported
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
            allow_global_pause: option::none(),
            variant: REGULATED_COIN_VERSION,
        };
}

/// Mark regulated state by showing the `DenyCapV2` object for the `Currency`.
public fun migrate_regulated_state_by_cap<T>(currency: &mut Currency<T>, cap: &DenyCapV2<T>) {
    currency.regulated =
        RegulatedState::Regulated {
            cap: object::id(cap),
            allow_global_pause: option::some(cap.allow_global_pause()),
            variant: REGULATED_COIN_VERSION,
        };
}

// === Borrowing of legacy CoinMetadata ===

/// Borrow the legacy `CoinMetadata` from a new `Currency`. To preserve the `ID`
/// of the legacy `CoinMetadata`, we create it on request and then store it as a
/// dynamic field for future borrows.
///
/// `Borrow<T>` ensures that the `CoinMetadata` is returned in the same transaction.
public fun borrow_legacy_metadata<T>(
    currency: &mut Currency<T>,
    ctx: &mut TxContext,
): (CoinMetadata<T>, Borrow<T>) {
    assert!(!currency.is_migrated_from_legacy(), EBorrowLegacyMetadata);

    if (!df::exists_(&currency.id, LegacyMetadataKey())) {
        let legacy = currency.to_legacy_metadata(ctx);
        df::add(&mut currency.id, LegacyMetadataKey(), legacy);
    };

    let mut legacy: CoinMetadata<T> = df::remove(&mut currency.id, LegacyMetadataKey());

    legacy.update_coin_metadata(
        currency.name,
        currency.symbol.to_ascii(),
        currency.description,
        currency.icon_url.to_ascii(),
    );

    (legacy, Borrow {})
}

/// Return the borrowed `CoinMetadata` and the `Borrow` potato to the `Currency`.
///
/// Note to self: Borrow requirement prevents deletion through this method.
public fun return_borrowed_legacy_metadata<T>(
    currency: &mut Currency<T>,
    mut legacy: CoinMetadata<T>,
    borrow: Borrow<T>,
    _ctx: &mut TxContext,
) {
    assert!(!df::exists_(&currency.id, LegacyMetadataKey()), EDuplicateBorrow);

    let Borrow {} = borrow;

    // Always store up to date value.
    legacy.update_coin_metadata(
        currency.name,
        currency.symbol.to_ascii(),
        currency.description,
        currency.icon_url.to_ascii(),
    );

    df::add(&mut currency.id, LegacyMetadataKey(), legacy);
}

// === Public getters  ===

/// Get the number of decimal places for the coin type.
public fun decimals<T>(currency: &Currency<T>): u8 { currency.decimals }

/// Get the human-readable name of the coin.
public fun name<T>(currency: &Currency<T>): String { currency.name }

/// Get the symbol/ticker of the coin.
public fun symbol<T>(currency: &Currency<T>): String { currency.symbol }

/// Get the description of the coin.
public fun description<T>(currency: &Currency<T>): String { currency.description }

/// Get the icon URL for the coin.
public fun icon_url<T>(currency: &Currency<T>): String { currency.icon_url }

/// Check if the metadata capability has been claimed for this `Currency` type.
public fun is_metadata_cap_claimed<T>(currency: &Currency<T>): bool {
    match (currency.metadata_cap_id) {
        MetadataCapState::Claimed(_) | MetadataCapState::Deleted => true,
        _ => false,
    }
}

/// Check if the metadata capability has been deleted for this `Currency` type.
public fun is_metadata_cap_deleted<T>(currency: &Currency<T>): bool {
    match (currency.metadata_cap_id) {
        MetadataCapState::Deleted => true,
        _ => false,
    }
}

/// Get the metadata cap ID, or none if it has not been claimed.
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
/// Returns `None` if:
/// - The `Currency` is not regulated;
/// - The `Currency` is migrated from legacy, and its regulated state has not been set;
public fun deny_cap_id<T>(currency: &Currency<T>): Option<ID> {
    match (currency.regulated) {
        RegulatedState::Regulated { cap, .. } => option::some(cap),
        RegulatedState::Unregulated | RegulatedState::Unknown => option::none(),
    }
}

/// Check if the supply is fixed.
public fun is_supply_fixed<T>(currency: &Currency<T>): bool {
    match (currency.supply.borrow()) {
        SupplyState::Fixed(_) => true,
        _ => false,
    }
}

/// Check if the supply is burn-only.
public fun is_supply_burn_only<T>(currency: &Currency<T>): bool {
    match (currency.supply.borrow()) {
        SupplyState::BurnOnly(_) => true,
        _ => false,
    }
}

/// Check if the currency is regulated.
public fun is_regulated<T>(currency: &Currency<T>): bool {
    match (currency.regulated) {
        RegulatedState::Regulated { .. } => true,
        _ => false,
    }
}

/// Get the total supply for the `Currency<T>` if the Supply is in fixed or
/// burn-only state. Returns `None` if the SupplyState is Unknown.
public fun total_supply<T>(currency: &Currency<T>): Option<u64> {
    match (currency.supply.borrow()) {
        SupplyState::Fixed(supply) => option::some(supply.value()),
        SupplyState::BurnOnly(supply) => option::some(supply.value()),
        SupplyState::Unknown => option::none(),
    }
}

/// Check if coin data exists for the given type T in the registry.
public fun exists<T>(registry: &CoinRegistry): bool {
    derived_object::exists(&registry.id, CurrencyKey<T>())
}

/// Whether the currency is migrated from legacy.
fun is_migrated_from_legacy<T>(currency: &Currency<T>): bool {
    !currency.extra_fields.contains(&NEW_CURRENCY_MARKER.to_string())
}

/// Create a new legacy `CoinMetadata` from a `Currency`.
fun to_legacy_metadata<T>(currency: &Currency<T>, ctx: &mut TxContext): CoinMetadata<T> {
    coin::new_coin_metadata(
        currency.decimals,
        currency.name,
        currency.symbol.to_ascii(),
        currency.description,
        currency.icon_url.to_ascii(),
        ctx,
    )
}

#[allow(unused_function)]
/// Create and share the singleton `CoinRegistry` -- this function is
/// called exactly once, during the upgrade epoch.
/// Only the system address (0x0) can create the registry.
fun create(ctx: &TxContext) {
    assert!(ctx.sender() == @0x0, ENotSystemAddress);

    transfer::share_object(CoinRegistry {
        id: object::sui_coin_registry_object_id(),
    });
}

/// Internal macro to keep implementation between build and test modes.
macro fun finalize_impl<$T>(
    $builder: CurrencyInitializer<$T>,
    $ctx: &mut TxContext,
): (Currency<$T>, MetadataCap<$T>) {
    let CurrencyInitializer { mut currency, extra_fields, is_otw: _ } = $builder;
    extra_fields.destroy_empty();
    let id = object::new($ctx);
    currency.metadata_cap_id = MetadataCapState::Claimed(id.to_inner());

    // Mark the currency as new, so in the future we can support borrowing of the
    // legacy metadata.
    currency
        .extra_fields
        .insert(
            NEW_CURRENCY_MARKER.to_string(),
            ExtraField(type_name::with_original_ids<bool>(), NEW_CURRENCY_MARKER),
        );

    (currency, MetadataCap<$T> { id })
}

/// Internal macro to keep implementation between build and test modes.
macro fun migrate_legacy_metadata_impl<$T>(
    $registry: &mut CoinRegistry,
    $legacy: &CoinMetadata<$T>,
): Currency<$T> {
    let registry = $registry;
    let legacy = $legacy;

    assert!(!registry.exists<$T>(), ECurrencyAlreadyRegistered);
    assert!(is_ascii_printable!(&legacy.get_symbol().to_string()), EInvalidSymbol);

    Currency<$T> {
        id: derived_object::claim(&mut registry.id, CurrencyKey<$T>()),
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
/// For transactional tests (if CoinRegistry is used as a shared object).
public fun share_for_testing(registry: CoinRegistry) {
    transfer::share_object(registry);
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
    builder: CurrencyInitializer<T>,
    ctx: &mut TxContext,
): (Currency<T>, MetadataCap<T>) {
    finalize_impl!(builder, ctx)
}

#[test_only]
public fun migrate_legacy_metadata_for_testing<T>(
    registry: &mut CoinRegistry,
    legacy: &CoinMetadata<T>,
    _ctx: &mut TxContext,
): Currency<T> {
    migrate_legacy_metadata_impl!(registry, legacy)
}
