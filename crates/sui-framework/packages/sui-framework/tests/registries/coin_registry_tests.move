// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

#[test_only]
module sui::coin_registry_tests;

use std::string::String;
use std::unit_test::{assert_eq, destroy};
use sui::coin::{Self, DenyCapV2, TreasuryCap, CoinMetadata};
use sui::coin_registry::{Self, Currency, CurrencyInitializer, CoinRegistry};
use sui::test_scenario;
use sui::url;

/// OTW-like.
public struct COIN_REGISTRY_TESTS has drop {}

/// Dynamic currency creation.
public struct TestDynamic has key { id: UID }

#[test]
fun default_scenario() {
    let ctx = &mut tx_context::dummy();
    let (builder, t_cap) = new_builder().build_otw(COIN_REGISTRY_TESTS {}, ctx);
    let (currency, metadata_cap) = builder.finalize_unwrap_for_testing(ctx);

    // Treasury and Metadata Caps registered properly.
    assert!(currency.treasury_cap_id().is_some_and!(|id| id == object::id(&t_cap)));
    assert!(currency.metadata_cap_id().is_some_and!(|id| id == object::id(&metadata_cap)));
    assert!(currency.is_metadata_cap_claimed());
    assert!(currency.total_supply().is_none());

    // Check metadata parameters (ignored in other tests!)
    assert_eq!(currency.decimals(), DECIMALS);
    assert_eq!(currency.symbol(), SYMBOL.to_string());
    assert_eq!(currency.name(), NAME.to_string());
    assert_eq!(currency.description(), DESCRIPTION.to_string());
    assert_eq!(currency.icon_url(), ICON_URL.to_string());

    // Check supply state.
    assert!(!currency.is_supply_fixed());
    assert!(!currency.is_supply_burn_only());
    assert!(!currency.is_regulated());

    destroy(metadata_cap);
    destroy(currency);
    destroy(t_cap);
}

// === Regulated Currency ===

#[test]
fun regulated_default() {
    let ctx = &mut tx_context::dummy();
    let (mut builder, t_cap) = new_builder().build_otw(COIN_REGISTRY_TESTS {}, ctx);
    let deny_cap = builder.make_regulated(true, ctx);
    let (currency, metadata_cap) = builder.finalize_unwrap_for_testing(ctx);

    assert!(currency.is_regulated());
    assert!(currency.deny_cap_id().is_some_and!(|id| id == object::id(&deny_cap)));

    destroy(metadata_cap);
    destroy(deny_cap);
    destroy(currency);
    destroy(t_cap);
}

#[test, expected_failure(abort_code = coin_registry::EDenyCapAlreadyCreated)]
fun regulated_twice_fail() {
    let ctx = &mut tx_context::dummy();
    let (mut builder, _t_cap) = new_builder().build_otw(COIN_REGISTRY_TESTS {}, ctx);
    let _deny_cap = builder.make_regulated(true, ctx);
    let _deny_cap2 = builder.make_regulated(true, ctx);

    abort
}

// === Metadata Updates & Metadata Cap States ===

#[test]
fun update_metadata() {
    let ctx = &mut tx_context::dummy();
    let (builder, t_cap) = new_builder().build_otw(COIN_REGISTRY_TESTS {}, ctx);
    let (mut currency, metadata_cap) = builder.finalize_unwrap_for_testing(ctx);

    // Perform updates on metadata.
    currency.set_name(&metadata_cap, b"new_name".to_string());
    currency.set_description(&metadata_cap, b"new_description".to_string());
    currency.set_icon_url(&metadata_cap, b"new_icon_url".to_string());

    assert_eq!(currency.name(), b"new_name".to_string());
    assert_eq!(currency.description(), b"new_description".to_string());
    assert_eq!(currency.icon_url(), b"new_icon_url".to_string());

    destroy(metadata_cap);
    destroy(currency);
    destroy(t_cap);
}

#[test, expected_failure(abort_code = coin_registry::EInvalidSymbol)]
fun create_symbol_non_ascii() {
    let ctx = &mut tx_context::dummy();
    let (_builder, _t_cap) = new_builder()
        .symbol(b"\x00".to_string())
        .build_otw(COIN_REGISTRY_TESTS {}, ctx);

    abort
}

#[test]
fun delete_metadata_cap() {
    let ctx = &mut tx_context::dummy();
    let (builder, t_cap) = new_builder().build_otw(COIN_REGISTRY_TESTS {}, ctx);
    let (mut currency, metadata_cap) = builder.finalize_unwrap_for_testing(ctx);

    currency.delete_metadata_cap(metadata_cap);

    destroy(currency);
    destroy(t_cap);
}

// === Supply States ===

#[test]
// Scenario:
// 1. create a new Currency and mint some coins
// 2. make the supply fixed
// 3. check the total supply value
fun fixed_supply() {
    let ctx = &mut tx_context::dummy();
    let (builder, mut t_cap) = new_builder().build_otw(COIN_REGISTRY_TESTS {}, ctx);
    let (mut currency, metadata_cap) = builder.finalize_unwrap_for_testing(ctx);

    assert!(currency.total_supply().is_none());
    assert!(!currency.is_supply_fixed());
    assert!(!currency.is_supply_burn_only());

    let amount = 10_000;
    t_cap.mint(amount, ctx).burn_for_testing();
    currency.make_supply_fixed(t_cap);

    assert!(!currency.is_supply_burn_only());
    assert!(currency.total_supply().is_some_and!(|total| total == amount));
    assert!(currency.is_supply_fixed());

    destroy(metadata_cap);
    destroy(currency);
}

#[test, expected_failure(abort_code = coin_registry::ESupplyNotBurnOnly)]
fun burn_fixed_supply() {
    let ctx = &mut tx_context::dummy();
    let (builder, mut t_cap) = new_builder().build_otw(COIN_REGISTRY_TESTS {}, ctx);
    let (mut currency, _metadata_cap) = builder.finalize_unwrap_for_testing(ctx);
    let coins = t_cap.mint(10_000, ctx);

    currency.make_supply_fixed(t_cap);
    currency.burn(coins);

    abort
}

#[test, expected_failure(abort_code = coin_registry::ESupplyNotBurnOnly)]
fun burn_unknown_supply() {
    let ctx = &mut tx_context::dummy();
    let (builder, mut t_cap) = new_builder().build_otw(COIN_REGISTRY_TESTS {}, ctx);
    let (mut currency, _metadata_cap) = builder.finalize_unwrap_for_testing(ctx);

    currency.burn(t_cap.mint(10_000, ctx));

    abort
}

#[test, expected_failure(abort_code = coin_registry::ESupplyNotBurnOnly)]
fun burn_balance_fixed_supply() {
    let ctx = &mut tx_context::dummy();
    let (builder, mut t_cap) = new_builder().build_otw(COIN_REGISTRY_TESTS {}, ctx);
    let (mut currency, _metadata_cap) = builder.finalize_unwrap_for_testing(ctx);
    let coins = t_cap.mint(10_000, ctx);

    currency.make_supply_fixed(t_cap);
    currency.burn_balance(coins.into_balance());

    abort
}

#[test, expected_failure(abort_code = coin_registry::ESupplyNotBurnOnly)]
fun burn_balance_unknown_supply() {
    let ctx = &mut tx_context::dummy();
    let (builder, mut t_cap) = new_builder().build_otw(COIN_REGISTRY_TESTS {}, ctx);
    let (mut currency, _metadata_cap) = builder.finalize_unwrap_for_testing(ctx);

    currency.burn_balance(t_cap.mint(10_000, ctx).into_balance());

    abort
}

#[test]
// Scenario:
// 1. create a new Currency and mint some coins
// 2. make the supply burn-only
// 3. burn all coins
fun burn_only_supply() {
    let ctx = &mut tx_context::dummy();
    let (builder, mut t_cap) = new_builder().build_otw(COIN_REGISTRY_TESTS {}, ctx);
    let (mut currency, metadata_cap) = builder.finalize_unwrap_for_testing(ctx);

    assert!(currency.total_supply().is_none());
    assert!(!currency.is_supply_fixed());
    assert!(!currency.is_supply_burn_only());

    let amount = 10_000;
    let mut coins = t_cap.mint(amount, ctx);
    currency.make_supply_burn_only(t_cap);

    assert!(!currency.is_supply_fixed());
    assert!(currency.total_supply().is_some_and!(|total| total == amount));
    assert!(currency.is_supply_burn_only());

    // Perform a burn operation on burn-only supply.
    // And check the total supply value.
    currency.burn(coins.split(5_000, ctx));
    currency.burn_balance(coins.into_balance()); // Another way to burn.
    assert!(currency.total_supply().is_some_and!(|total| total == 0));

    destroy(metadata_cap);
    destroy(currency);
}

// === Dynamic Currency Creation ===

#[test]
fun dynamic_currency_default() {
    let ctx = &mut tx_context::dummy();
    let mut registry = coin_registry::create_coin_data_registry_for_testing(ctx);
    let (builder, t_cap) = new_builder().build_dynamic(&mut registry, ctx);
    let (currency, metadata_cap) = builder.finalize_unwrap_for_testing(ctx);

    assert!(registry.exists<TestDynamic>());
    assert!(currency.is_metadata_cap_claimed());
    assert!(!currency.is_supply_fixed());
    assert!(!currency.is_supply_burn_only());
    assert!(!currency.is_regulated());

    // Check metadata parameters (ignored in other tests!)
    assert_eq!(currency.decimals(), DECIMALS);
    assert_eq!(currency.symbol(), SYMBOL.to_string());
    assert_eq!(currency.name(), NAME.to_string());
    assert_eq!(currency.description(), DESCRIPTION.to_string());
    assert_eq!(currency.icon_url(), ICON_URL.to_string());

    destroy(metadata_cap);
    destroy(registry);
    destroy(currency);
    destroy(t_cap);
}

#[test, expected_failure(abort_code = coin_registry::ECurrencyAlreadyExists)]
fun dynamic_currency_duplicate() {
    let ctx = &mut tx_context::dummy();
    let mut registry = coin_registry::create_coin_data_registry_for_testing(ctx);
    let (_builder, _t_cap) = new_builder().build_dynamic(&mut registry, ctx);
    let (_builder2, _t_cap2) = new_builder().build_dynamic(&mut registry, ctx);

    abort
}

// === Migration from Legacy ===

#[test]
fun perfect_migration_regulated() {
    let ctx = &mut tx_context::dummy();
    let mut registry = coin_registry::create_coin_data_registry_for_testing(ctx);
    let (t_cap, deny_cap, mut metadata) = new_builder().build_legacy_regulated(
        COIN_REGISTRY_TESTS {},
        ctx,
    );
    let mut currency = coin_registry::migrate_legacy_metadata_for_testing(
        &mut registry,
        &metadata,
        ctx,
    );

    // Ensure migration correctness.
    assert_eq!(currency.decimals(), DECIMALS);
    assert_eq!(currency.symbol(), SYMBOL.to_string());
    assert_eq!(currency.name(), NAME.to_string());
    assert_eq!(currency.description(), DESCRIPTION.to_string());
    assert_eq!(currency.icon_url(), ICON_URL.to_string());

    assert!(!currency.is_metadata_cap_claimed());
    assert!(!currency.is_regulated());

    // Mark as regulated with DenyCapV2.
    currency.migrate_regulated_state_by_cap(&deny_cap);
    assert!(currency.is_regulated());
    assert!(currency.deny_cap_id().is_some_and!(|id| id == object::id(&deny_cap)));

    // Make an adjustment to the original metadata and refresh the currency
    // state through it.
    t_cap.update_description(&mut metadata, b"New description".to_string());
    t_cap.update_icon_url(&mut metadata, b"https://new.test.com/img.png".to_ascii_string());
    t_cap.update_name(&mut metadata, b"New name".to_string());
    t_cap.update_symbol(&mut metadata, b"NEW_TEST".to_ascii_string());

    // Perform a permissionless update before claiming the metadata cap.
    currency.update_from_legacy_metadata(&metadata);

    // Check that the updates were applied.
    assert_eq!(currency.description(), b"New description".to_string());
    assert_eq!(currency.icon_url(), b"https://new.test.com/img.png".to_string());
    assert_eq!(currency.name(), b"New name".to_string());
    assert_eq!(currency.symbol(), b"NEW_TEST".to_string());
    assert_eq!(currency.decimals(), DECIMALS);

    // Now updates can be made via the registry.
    let metadata_cap = currency.claim_metadata_cap(&t_cap, ctx);
    assert!(currency.is_metadata_cap_claimed());
    assert!(currency.metadata_cap_id().is_some_and!(|id| id == object::id(&metadata_cap)));

    destroy(metadata_cap);
    destroy(registry);
    destroy(currency);
    destroy(deny_cap);
    destroy(metadata);
    destroy(t_cap);
}

#[test]
fun perfect_migration_with_regulated_coin_metadata() {
    let ctx = &mut tx_context::dummy();
    let mut registry = coin_registry::create_coin_data_registry_for_testing(ctx);
    let (t_cap, deny_cap, metadata) = new_builder().build_legacy_regulated(
        COIN_REGISTRY_TESTS {},
        ctx,
    );
    let mut currency = coin_registry::migrate_legacy_metadata_for_testing(
        &mut registry,
        &metadata,
        ctx,
    );

    let regulated_coin_metadata = coin::regulated_coin_metadata_for_testing(
        object::id(&metadata),
        object::id(&deny_cap),
        ctx,
    );

    currency.migrate_regulated_state_by_metadata(&regulated_coin_metadata);

    assert!(currency.is_regulated());
    assert!(currency.deny_cap_id().is_some_and!(|id| id == object::id(&deny_cap)));

    destroy(regulated_coin_metadata);
    destroy(registry);
    destroy(currency);
    destroy(deny_cap);
    destroy(metadata);
    destroy(t_cap);
}

#[test, expected_failure(abort_code = coin_registry::EDenyListStateAlreadySet)]
fun migrate_regulated_state_by_metadata_twice() {
    let ctx = &mut tx_context::dummy();
    let mut registry = coin_registry::create_coin_data_registry_for_testing(ctx);
    let (_t_cap, deny_cap, metadata) = new_builder().build_legacy_regulated(
        COIN_REGISTRY_TESTS {},
        ctx,
    );

    let mut currency = coin_registry::migrate_legacy_metadata_for_testing(
        &mut registry,
        &metadata,
        ctx,
    );

    let regulated_coin_metadata = coin::regulated_coin_metadata_for_testing(
        object::id(&metadata),
        object::id(&deny_cap),
        ctx,
    );

    currency.migrate_regulated_state_by_metadata(&regulated_coin_metadata);
    currency.migrate_regulated_state_by_metadata(&regulated_coin_metadata);

    abort
}

#[test, expected_failure(abort_code = coin_registry::ECannotUpdateManagedMetadata)]
fun update_legacy_fail() {
    let ctx = &mut tx_context::dummy();
    let mut registry = coin_registry::create_coin_data_registry_for_testing(ctx);
    let (t_cap, _deny_cap, metadata) = new_builder().build_legacy_regulated(
        COIN_REGISTRY_TESTS {},
        ctx,
    );
    let mut currency = coin_registry::migrate_legacy_metadata_for_testing(
        &mut registry,
        &metadata,
        ctx,
    );

    let _metadata_cap = currency.claim_metadata_cap(&t_cap, ctx);
    currency.update_from_legacy_metadata(&metadata);

    abort
}

// === Borrow Legacy CoinMetadata ===

#[test]
// Scenario:
// 1. create a new Currency and finalize it
// 2. borrow the legacy metadata and check the values
// 3. mutate the legacy metadata
// 4. return the legacy metadata back to the currency
// 5. borrow the legacy metadata again and check the values
fun borrow_legacy_coin_metadata() {
    let ctx = &mut tx_context::dummy();
    let (builder, t_cap) = new_builder().build_otw(COIN_REGISTRY_TESTS {}, ctx);
    let (mut currency, metadata_cap) = builder.finalize_unwrap_for_testing(ctx);

    // Borrow the legacy metadata and check the values.
    let (mut legacy, borrow) = currency.borrow_legacy_metadata(ctx);
    let legacy_id = object::id(&legacy); // Ensure preservation of the object ID.

    assert_eq!(legacy.get_decimals(), currency.decimals());
    assert_eq!(legacy.get_symbol(), currency.symbol().to_ascii());
    assert_eq!(legacy.get_name(), currency.name());
    assert_eq!(legacy.get_description(), currency.description());
    assert_eq!(
        legacy.get_icon_url().destroy_or!(abort).inner_url(),
        currency.icon_url().to_ascii(),
    );

    // mutate legacy cm
    coin::update_name(&t_cap, &mut legacy, b"New name".to_string());

    // Return and borrow once again.
    currency.return_borrowed_legacy_metadata(legacy, borrow, ctx);
    let (legacy, borrow) = currency.borrow_legacy_metadata(ctx);

    // Change in legacy CM were wiped clean to be in sync with the currency.
    assert_eq!(legacy.get_name(), currency.name());
    assert_eq!(object::id(&legacy), legacy_id); // ID is preserved!

    // Return it back to the currency.
    currency.return_borrowed_legacy_metadata(legacy, borrow, ctx);

    destroy(t_cap);
    destroy(currency);
    destroy(metadata_cap);
}

#[test, expected_failure(abort_code = coin_registry::EBorrowLegacyMetadata)]
fun try_borrowing_from_migrated_currency_fail() {
    let ctx = &mut tx_context::dummy();
    let mut registry = coin_registry::create_coin_data_registry_for_testing(ctx);
    let (_t_cap, legacy) = new_builder().build_legacy(COIN_REGISTRY_TESTS {}, ctx);
    let mut currency = registry.migrate_legacy_metadata_for_testing(&legacy, ctx);
    let (_borrowed_legacy, _borrow) = currency.borrow_legacy_metadata(ctx);

    abort
}

#[test, expected_failure(abort_code = coin_registry::EDuplicateBorrow)]
fun borrow_legacy_coin_metadata_twice() {
    let ctx = &mut tx_context::dummy();
    let (builder, _t_cap) = new_builder().build_otw(COIN_REGISTRY_TESTS {}, ctx);
    let (mut currency, _metadata_cap) = builder.finalize_unwrap_for_testing(ctx);

    let (legacy_1, borrow_1) = currency.borrow_legacy_metadata(ctx);
    let (legacy_2, borrow_2) = currency.borrow_legacy_metadata(ctx);

    currency.return_borrowed_legacy_metadata(legacy_1, borrow_1, ctx);
    currency.return_borrowed_legacy_metadata(legacy_2, borrow_2, ctx);

    abort
}

// === Test Scenario + Receiving ===

#[test]
fun otw_currency_promotion() {
    let mut test = test_scenario::begin(@0);
    let (builder, t_cap) = new_builder().build_otw(COIN_REGISTRY_TESTS {}, test.ctx());
    let metadata_cap = builder.finalize(test.ctx());

    test.next_tx(@10);

    // Get Receiving<Currency<COIN_REGISTRY_TESTS>> from 0xC address
    let currency = test_scenario::most_recent_receiving_ticket<Currency<COIN_REGISTRY_TESTS>>(
        &object::sui_coin_registry_address().to_id(),
    );

    destroy(metadata_cap);
    destroy(currency);
    destroy(t_cap);
    test.end();
}

#[test]
fun new_currency_is_shared() {
    let mut test = test_scenario::begin(@0);
    let mut registry = coin_registry::create_coin_data_registry_for_testing(test.ctx());
    let (builder, t_cap) = new_builder().build_dynamic(&mut registry, test.ctx());
    let metadata_cap = builder.finalize(test.ctx());

    test.next_tx(@0);

    let mut currency = test.take_shared<Currency<TestDynamic>>();
    currency.delete_metadata_cap(metadata_cap);
    test_scenario::return_shared(currency);

    destroy(registry);
    destroy(t_cap);

    test.end();
}

#[test]
fun new_currency_is_shared_and_metadata_cap_is_deleted() {
    let mut test = test_scenario::begin(@0);
    let mut registry = coin_registry::create_coin_data_registry_for_testing(test.ctx());
    let (builder, t_cap) = new_builder().build_dynamic(&mut registry, test.ctx());
    builder.finalize_and_delete_metadata_cap(test.ctx());

    test.next_tx(@0);

    let currency = test.take_shared<Currency<TestDynamic>>();
    assert!(currency.is_metadata_cap_claimed());
    assert!(currency.is_metadata_cap_deleted());
    test_scenario::return_shared(currency);

    destroy(registry);
    destroy(t_cap);

    test.end();
}

// === Metadata Builder ===

public struct MetadataBuilder has drop {
    decimals: u8,
    symbol: String,
    name: String,
    description: String,
    icon_url: String,
}

public fun new_builder(): MetadataBuilder {
    MetadataBuilder {
        decimals: DECIMALS,
        symbol: SYMBOL.to_string(),
        name: NAME.to_string(),
        description: DESCRIPTION.to_string(),
        icon_url: ICON_URL.to_string(),
    }
}

public fun symbol(mut b: MetadataBuilder, symbol: String): MetadataBuilder {
    b.symbol = symbol;
    b
}

public fun build_dynamic(
    b: MetadataBuilder,
    registry: &mut CoinRegistry,
    ctx: &mut TxContext,
): (CurrencyInitializer<TestDynamic>, TreasuryCap<TestDynamic>) {
    registry.new_currency<TestDynamic>(
        b.decimals,
        b.symbol,
        b.name,
        b.description,
        b.icon_url,
        ctx,
    )
}

public fun build_otw<T: drop>(
    b: MetadataBuilder,
    otw: T,
    ctx: &mut TxContext,
): (CurrencyInitializer<T>, TreasuryCap<T>) {
    coin_registry::new_currency_with_otw(
        otw,
        b.decimals,
        b.symbol,
        b.name,
        b.description,
        b.icon_url,
        ctx,
    )
}

#[allow(deprecated_usage)]
public fun build_legacy<T: drop>(
    b: MetadataBuilder,
    otw: T,
    ctx: &mut TxContext,
): (TreasuryCap<T>, CoinMetadata<T>) {
    coin::create_currency(
        otw,
        b.decimals,
        b.symbol.into_bytes(),
        b.name.into_bytes(),
        b.description.into_bytes(),
        option::some(url::new_unsafe(b.icon_url.to_ascii())),
        ctx,
    )
}

#[allow(deprecated_usage)]
public fun build_legacy_regulated<T: drop>(
    b: MetadataBuilder,
    otw: T,
    ctx: &mut TxContext,
): (TreasuryCap<T>, DenyCapV2<T>, CoinMetadata<T>) {
    coin::create_regulated_currency_v2(
        otw,
        b.decimals,
        b.symbol.into_bytes(),
        b.name.into_bytes(),
        b.description.into_bytes(),
        option::some(url::new_unsafe(b.icon_url.to_ascii())),
        false,
        ctx,
    )
}

// === Default Values ===

const DECIMALS: u8 = 6;
const SYMBOL: vector<u8> = b"TEST";
const NAME: vector<u8> = b"Test";
const DESCRIPTION: vector<u8> = b"Test";
const ICON_URL: vector<u8> = b"https://test.com/img.png";
