// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

#[test_only]
module sui::coin_registry_tests;

use sui::coin::{Self, TreasuryCap, DenyCapV2, CoinMetadata, RegulatedCoinMetadata};
use sui::coin_registry::{Self, CoinRegistry, MetadataCap, InitCoinData, CoinData};
use sui::test_scenario;
use sui::url;

public struct COIN_REGISTRY_TESTS has drop {}

const TEST_ADDR: address = @0xA11CE;

fun create_test_coin(
    scenario: &mut test_scenario::Scenario,
): (
    TreasuryCap<COIN_REGISTRY_TESTS>,
    MetadataCap<COIN_REGISTRY_TESTS>,
    InitCoinData<COIN_REGISTRY_TESTS>,
) {
    let witness = COIN_REGISTRY_TESTS {};
    coin::create_currency_v2(
        witness,
        6,
        b"COIN_REGISTRY_TESTS".to_string(),
        b"coin_name".to_string(),
        b"description".to_string(),
        b"test_url".to_string(),
        scenario.ctx(),
    )
}

fun create_test_regulated_coin(
    scenario: &mut test_scenario::Scenario,
): (
    TreasuryCap<COIN_REGISTRY_TESTS>,
    MetadataCap<COIN_REGISTRY_TESTS>,
    DenyCapV2<COIN_REGISTRY_TESTS>,
    InitCoinData<COIN_REGISTRY_TESTS>,
) {
    let witness = COIN_REGISTRY_TESTS {};
    coin::create_regulated_currency_v3(
        witness,
        6,
        b"REGULATED_COIN_METADATA_TESTS".to_string(),
        b"coin_name".to_string(),
        b"description".to_string(),
        b"test_url".to_string(),
        true,
        scenario.ctx(),
    )
}

#[allow(deprecated_usage)]
fun create_test_coin_v1(
    scenario: &mut test_scenario::Scenario,
): (TreasuryCap<COIN_REGISTRY_TESTS>, CoinMetadata<COIN_REGISTRY_TESTS>) {
    let witness = COIN_REGISTRY_TESTS {};
    coin::create_currency(
        witness,
        6,
        b"COIN_TESTS",
        b"coin_name",
        b"description",
        option::some(url::new_unsafe_from_bytes(b"icon_url")),
        scenario.ctx(),
    )
}

#[allow(deprecated_usage)]
fun create_test_regulated_coin_v2(
    scenario: &mut test_scenario::Scenario,
): (
    TreasuryCap<COIN_REGISTRY_TESTS>,
    DenyCapV2<COIN_REGISTRY_TESTS>,
    CoinMetadata<COIN_REGISTRY_TESTS>,
) {
    let witness = COIN_REGISTRY_TESTS {};
    coin::create_regulated_currency_v2<COIN_REGISTRY_TESTS>(
        witness,
        6,
        b"COIN_TESTS",
        b"coin_name",
        b"description",
        option::some(url::new_unsafe_from_bytes(b"icon_url")),
        /* allow_global_pause */ true,
        scenario.ctx(),
    )
}

fun initialize_test_registry(scenario: &mut test_scenario::Scenario): CoinRegistry {
    scenario.next_tx(@0x0);
    coin_registry::create_coin_data_registry_for_testing(scenario.ctx())
}

#[test]
fun test_register_metadata() {
    let mut scenario = test_scenario::begin(TEST_ADDR);
    let (treasury_cap, metadata_cap, init_coin_data) = create_test_coin(&mut scenario);

    let mut registry = initialize_test_registry(&mut scenario);
    scenario.next_tx(@0x30);

    let coin_data = init_coin_data.unwrap_for_testing();

    registry.register_coin_data(coin_data);

    transfer::public_transfer(treasury_cap, scenario.ctx().sender());
    transfer::public_transfer(metadata_cap, scenario.ctx().sender());
    transfer::public_transfer(registry, scenario.ctx().sender());

    scenario.end();
}

#[test]
fun test_migrate_receiving() {
    let mut scenario = test_scenario::begin(TEST_ADDR);
    let (treasury_cap, metadata_cap, init_coin_data) = create_test_coin(&mut scenario);

    let mut registry = initialize_test_registry(&mut scenario);
    scenario.next_tx(@0x20);

    let coin_data = init_coin_data.unwrap_for_testing();

    transfer::public_transfer(coin_data, registry.id().to_address());

    scenario.next_tx(@0x20);

    let receiving_coin_data_ids = test_scenario::receivable_object_ids_for_owner_id<
        CoinData<COIN_REGISTRY_TESTS>,
    >(
        object::id(&registry),
    );

    let ticket = test_scenario::receiving_ticket_by_id<
        CoinData<COIN_REGISTRY_TESTS>,
    >(receiving_coin_data_ids[0]);

    registry.migrate_receiving(ticket);

    scenario.next_tx(@0x20);

    assert!(registry.exists<COIN_REGISTRY_TESTS>());

    transfer::public_transfer(treasury_cap, scenario.ctx().sender());
    transfer::public_transfer(metadata_cap, scenario.ctx().sender());
    transfer::public_transfer(registry, scenario.ctx().sender());

    scenario.end();
}

#[test]
fun test_setters() {
    let mut scenario = test_scenario::begin(TEST_ADDR);
    let (treasury_cap, metadata_cap, init_coin_data) = create_test_coin(&mut scenario);

    let mut registry = initialize_test_registry(&mut scenario);
    scenario.next_tx(@0x20);

    let coin_data = init_coin_data.unwrap_for_testing();

    registry.register_coin_data(coin_data);

    registry.set_name(&metadata_cap, b"test".to_string());
    registry.set_symbol(&metadata_cap, b"TEST".to_string());
    registry.set_description(&metadata_cap, b"test description".to_string());
    registry.set_icon_url(&metadata_cap, b"https://example.com/icon.png".to_string());

    assert!(registry.data<COIN_REGISTRY_TESTS>().name() == b"test".to_string());
    assert!(registry.data<COIN_REGISTRY_TESTS>().symbol() == b"TEST".to_string());
    assert!(registry.data<COIN_REGISTRY_TESTS>().description() == b"test description".to_string());
    assert!(
        registry.data<COIN_REGISTRY_TESTS>().icon_url() == b"https://example.com/icon.png".to_string(),
    );

    transfer::public_transfer(treasury_cap, scenario.ctx().sender());
    transfer::public_transfer(metadata_cap, scenario.ctx().sender());
    transfer::public_transfer(registry, scenario.ctx().sender());

    scenario.end();
}

#[test, expected_failure(abort_code = ::sui::coin::EMetadataCapNotClaimed)]
fun test_freeze_supply_cap_not_claimed_abort() {
    let mut scenario = test_scenario::begin(TEST_ADDR);
    let (treasury_cap, metadata) = create_test_coin_v1(&mut scenario);

    let mut registry = initialize_test_registry(&mut scenario);
    scenario.next_tx(@0x20);

    scenario.next_tx(@0x20);

    coin::migrate_metadata_to_registry<COIN_REGISTRY_TESTS>(
        &mut registry,
        metadata,
        scenario.ctx(),
    );

    coin::register_supply(&mut registry, treasury_cap);

    transfer::public_transfer(registry, scenario.ctx().sender());

    scenario.end();
}

#[test, expected_failure(abort_code = ::sui::coin::EMetadataNotFound)]
fun test_freeze_supply_metadata_not_found_abort() {
    let mut scenario = test_scenario::begin(TEST_ADDR);
    let (treasury_cap, metadata_cap, init_coin_data) = create_test_coin(&mut scenario);

    let mut registry = initialize_test_registry(&mut scenario);
    scenario.next_tx(@0x20);

    coin::register_supply(&mut registry, treasury_cap);

    transfer::public_transfer(metadata_cap, scenario.ctx().sender());
    transfer::public_transfer(registry, scenario.ctx().sender());
    let coin_data = init_coin_data.unwrap_for_testing();
    transfer::public_transfer(coin_data, scenario.sender());

    scenario.end();
}

#[test]
fun test_register_supply() {
    let mut scenario = test_scenario::begin(TEST_ADDR);
    let (treasury_cap, metadata_cap, init_coin_data) = create_test_coin(&mut scenario);

    let mut registry = initialize_test_registry(&mut scenario);
    scenario.next_tx(@0x20);

    let coin_data = init_coin_data.unwrap_for_testing();

    registry.register_coin_data(coin_data);

    coin::register_supply(&mut registry, treasury_cap);

    assert!(registry.data<COIN_REGISTRY_TESTS>().supply_registered());

    transfer::public_transfer(metadata_cap, scenario.ctx().sender());
    transfer::public_transfer(registry, scenario.ctx().sender());

    scenario.end();
}

#[test]
fun test_init_freeze_supply() {
    let mut scenario = test_scenario::begin(TEST_ADDR);
    let (treasury_cap, metadata_cap, mut init_coin_data) = create_test_coin(&mut scenario);

    let mut registry = initialize_test_registry(&mut scenario);
    scenario.next_tx(@0x20);

    coin::init_register_supply(&mut init_coin_data, treasury_cap);

    let coin_data = init_coin_data.unwrap_for_testing();

    registry.register_coin_data(coin_data);

    assert!(registry.data<COIN_REGISTRY_TESTS>().supply_registered());

    transfer::public_transfer(metadata_cap, scenario.ctx().sender());
    transfer::public_transfer(registry, scenario.ctx().sender());

    scenario.end();
}

#[test]
fun test_set_decimals() {
    let mut scenario = test_scenario::begin(TEST_ADDR);
    let (treasury_cap, metadata_cap, init_coin_data) = create_test_coin(&mut scenario);

    let mut registry = initialize_test_registry(&mut scenario);
    scenario.next_tx(@0x20);

    let mut coin_data = init_coin_data.unwrap_for_testing();

    coin_data.set_decimals(1);

    registry.register_coin_data(coin_data);

    assert!(registry.data<COIN_REGISTRY_TESTS>().decimals() == 1);

    transfer::public_transfer(treasury_cap, scenario.ctx().sender());
    transfer::public_transfer(metadata_cap, scenario.ctx().sender());
    transfer::public_transfer(registry, scenario.ctx().sender());

    scenario.end();
}

#[test]
fun test_migrate_regulated_metadata_to_registry() {
    let mut scenario = test_scenario::begin(TEST_ADDR);
    let (treasury_cap, metadata_cap, deny_cap, init_coin_data) = create_test_regulated_coin(
        &mut scenario,
    );

    let mut registry = initialize_test_registry(&mut scenario);
    scenario.next_tx(@0x20);

    let coin_data = init_coin_data.unwrap_for_testing();

    registry.register_coin_data<COIN_REGISTRY_TESTS>(coin_data);

    assert!(registry.data<COIN_REGISTRY_TESTS>().deny_cap() == option::some(object::id(&deny_cap)));

    transfer::public_transfer(treasury_cap, scenario.ctx().sender());
    transfer::public_transfer(deny_cap, scenario.ctx().sender());
    transfer::public_transfer(metadata_cap, scenario.ctx().sender());
    transfer::public_transfer(registry, scenario.ctx().sender());

    scenario.end();
}

#[test]
fun test_migration() {
    let mut scenario = test_scenario::begin(TEST_ADDR);
    let (treasury_cap, metadata) = create_test_coin_v1(
        &mut scenario,
    );

    let mut registry = initialize_test_registry(&mut scenario);
    scenario.next_tx(@0x20);

    coin::migrate_metadata_to_registry<COIN_REGISTRY_TESTS>(
        &mut registry,
        metadata,
        scenario.ctx(),
    );

    assert!(registry.data<COIN_REGISTRY_TESTS>().decimals() == 6);
    assert!(registry.data<COIN_REGISTRY_TESTS>().name() == b"coin_name".to_string());
    assert!(registry.data<COIN_REGISTRY_TESTS>().symbol() == b"COIN_TESTS".to_string());
    assert!(registry.data<COIN_REGISTRY_TESTS>().description() == b"description".to_string());
    assert!(registry.data<COIN_REGISTRY_TESTS>().icon_url() == b"icon_url".to_string());

    transfer::public_transfer(treasury_cap, scenario.ctx().sender());
    transfer::public_transfer(registry, scenario.ctx().sender());

    scenario.end();
}

#[test]
fun test_migrate_immutable_and_value() {
    let mut scenario = test_scenario::begin(TEST_ADDR);
    let (treasury_cap, metadata) = create_test_coin_v1(
        &mut scenario,
    );

    let mut registry = initialize_test_registry(&mut scenario);
    scenario.next_tx(@0x20);

    coin::migrate_immutable_metadata_to_registry<COIN_REGISTRY_TESTS>(
        &mut registry,
        &metadata,
        scenario.ctx(),
    );

    coin::migrate_metadata_to_registry<COIN_REGISTRY_TESTS>(
        &mut registry,
        metadata,
        scenario.ctx(),
    );

    assert!(registry.data<COIN_REGISTRY_TESTS>().decimals() == 6);
    assert!(registry.data<COIN_REGISTRY_TESTS>().name() == b"coin_name".to_string());
    assert!(registry.data<COIN_REGISTRY_TESTS>().symbol() == b"COIN_TESTS".to_string());
    assert!(registry.data<COIN_REGISTRY_TESTS>().description() == b"description".to_string());
    assert!(registry.data<COIN_REGISTRY_TESTS>().icon_url() == b"icon_url".to_string());

    transfer::public_transfer(treasury_cap, scenario.ctx().sender());
    transfer::public_transfer(registry, scenario.ctx().sender());

    scenario.end();
}

#[test]
fun test_migrate_regulated_to_registry() {
    let mut scenario = test_scenario::begin(TEST_ADDR);
    let (treasury_cap, deny_cap, metadata) = create_test_regulated_coin_v2(
        &mut scenario,
    );

    let mut registry = initialize_test_registry(&mut scenario);
    scenario.next_tx(@0x20);

    coin::migrate_metadata_to_registry<COIN_REGISTRY_TESTS>(
        &mut registry,
        metadata,
        scenario.ctx(),
    );

    let regulated_metadata_v1 = scenario.take_immutable<
        RegulatedCoinMetadata<COIN_REGISTRY_TESTS>,
    >();

    coin::migrate_regulated_metadata_to_registry<COIN_REGISTRY_TESTS>(
        &mut registry,
        &regulated_metadata_v1,
    );

    assert!(registry.data<COIN_REGISTRY_TESTS>().decimals() == 6);
    assert!(registry.data<COIN_REGISTRY_TESTS>().name() == b"coin_name".to_string());
    assert!(registry.data<COIN_REGISTRY_TESTS>().symbol() == b"COIN_TESTS".to_string());
    assert!(registry.data<COIN_REGISTRY_TESTS>().description() == b"description".to_string());
    assert!(registry.data<COIN_REGISTRY_TESTS>().icon_url() == b"icon_url".to_string());
    assert!(registry.data<COIN_REGISTRY_TESTS>().deny_cap() == option::some(object::id(&deny_cap)));

    transfer::public_transfer(treasury_cap, scenario.ctx().sender());
    transfer::public_transfer(deny_cap, scenario.ctx().sender());
    transfer::public_transfer(registry, scenario.ctx().sender());

    regulated_metadata_v1.freeze_for_testing();

    scenario.end();
}
