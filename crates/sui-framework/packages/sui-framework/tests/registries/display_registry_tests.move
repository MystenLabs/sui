// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

#[test_only]
module sui::display_registry_tests;

use std::unit_test::assert_eq;
use sui::display::{Self, new};
use sui::display_registry::{Self, DisplayRegistry, NewDisplay, SystemMigrationCap};
use sui::package::{Self, test_claim};
use sui::test_scenario::{Self, Scenario};
use sui::vec_map;

public struct MyKeyOnlyType has key { id: UID }
public struct MyPotato {}
public struct MY_OTW has drop {}

const DEMO_NAME_KEY: vector<u8> = b"name";
const DEMO_NAME_VALUE: vector<u8> = b"{name}";

#[test]
fun test_modern_creation_and_operations() {
    test_tx!(|registry, scenario| {
        let cap = registry.new<MyPotato>(vec_map::empty(), scenario.ctx());
        scenario.next_tx(@0x1);
        let mut display = scenario.take_shared<NewDisplay<MyPotato>>();
        assert_eq!(display.fields().length(), 0);

        display.add(&cap, DEMO_NAME_KEY.to_string(), DEMO_NAME_VALUE.to_string());
        test_scenario::return_shared(display);

        scenario.next_tx(@0x1);
        let display = scenario.take_shared<NewDisplay<MyPotato>>();
        assert_eq!(display.fields().length(), 1);
        assert_eq!(*display.fields().get(&DEMO_NAME_KEY.to_string()), DEMO_NAME_VALUE.to_string());
        test_scenario::return_shared(display);

        scenario.next_tx(@0x1);
        let mut display = scenario.take_shared<NewDisplay<MyPotato>>();
        display.remove(&cap, DEMO_NAME_KEY.to_string());
        test_scenario::return_shared(display);
        scenario.next_tx(@0x1);

        let display = scenario.take_shared<NewDisplay<MyPotato>>();
        assert_eq!(display.fields().length(), 0);
        test_scenario::return_shared(display);

        transfer::public_transfer(cap, @0x1);
    });
}

#[test]
fun test_legacy_claim() {
    test_tx!(|registry, scenario| {
        let publisher = package::test_claim(MY_OTW {}, scenario.ctx());
        let legacy_display = display::new<MyKeyOnlyType>(&publisher, scenario.ctx());
        // create a second legacy display so we can destroy it after claiming.
        let another_legacy_display = display::new<MyKeyOnlyType>(&publisher, scenario.ctx());

        let cap = take_migration_cap(scenario);

        // manually migrate `MyKeyOnlyType` to the new registry, as if we were the system.
        registry.migrate<MyKeyOnlyType>(&cap, vec_map::empty(), scenario.ctx());
        scenario.next_tx(@0x1);

        // Claim the display using our legacy display obj.
        let mut display = scenario.take_shared<NewDisplay<MyKeyOnlyType>>();
        let new_cap = display.claim(legacy_display, scenario.ctx());

        // use the cap to edit display!
        display.add(&new_cap, DEMO_NAME_KEY.to_string(), DEMO_NAME_VALUE.to_string());

        // After claiming, we can delete all other legacy displays permissionless-ly.
        display.delete_legacy(another_legacy_display);
        test_scenario::return_shared(display);
        transfer::public_transfer(new_cap, @0x1);

        cap.destroy_cap();
        publisher.burn();
    });
}

#[test]
fun test_legacy_claim_with_publisher() {
    test_tx!(|registry, scenario| {
        let mut publisher = package::test_claim(MY_OTW {}, scenario.ctx());
        let cap = take_migration_cap(scenario);
        registry.migrate<MyKeyOnlyType>(&cap, vec_map::empty(), scenario.ctx());
        scenario.next_tx(@0x1);

        let mut display = scenario.take_shared<NewDisplay<MyKeyOnlyType>>();
        let new_cap = display.claim_with_publisher(&mut publisher, scenario.ctx());
        display.add(&new_cap, DEMO_NAME_KEY.to_string(), DEMO_NAME_VALUE.to_string());
        test_scenario::return_shared(display);
        transfer::public_transfer(new_cap, @0x1);

        cap.destroy_cap();
        publisher.burn();
    });
}

#[test]
fun test_update_field() {
    test_tx!(|registry, scenario| {
        let cap = registry.new<MyKeyOnlyType>(vec_map::empty(), scenario.ctx());
        scenario.next_tx(@0x1);

        let mut display = scenario.take_shared<NewDisplay<MyKeyOnlyType>>();
        // Add `field` to display.
        display.add(&cap, DEMO_NAME_KEY.to_string(), DEMO_NAME_VALUE.to_string());
        assert_eq!(*display.fields().get(&DEMO_NAME_KEY.to_string()), DEMO_NAME_VALUE.to_string());

        // Update `field` with a new value.
        display.set(&cap, DEMO_NAME_KEY.to_string(), b"".to_string());
        assert_eq!(*display.fields().get(&DEMO_NAME_KEY.to_string()), b"".to_string());

        // call `set` for a fresh field (Should work!)
        display.set(&cap, b"new_field".to_string(), b"".to_string());
        assert_eq!(*display.fields().get(&b"new_field".to_string()), b"".to_string());

        transfer::public_transfer(cap, @0x1);
        test_scenario::return_shared(display);
    });
}

#[test, expected_failure(abort_code = display_registry::EDisplayAlreadyExists), allow(dead_code)]
fun test_display_already_exists() {
    test_tx!(|registry, scenario| {
        let _cap = registry.new<MyKeyOnlyType>(vec_map::empty(), scenario.ctx());
        let __cap = registry.new<MyKeyOnlyType>(vec_map::empty(), scenario.ctx());
        abort
    });
}

#[test, expected_failure(abort_code = display_registry::EDisplayAlreadyExists), allow(dead_code)]
fun test_migrate_twice() {
    test_tx!(|registry, scenario| {
        let cap = take_migration_cap(scenario);
        registry.migrate<MyKeyOnlyType>(&cap, vec_map::empty(), scenario.ctx());
        registry.migrate<MyKeyOnlyType>(&cap, vec_map::empty(), scenario.ctx());
        abort
    });
}

#[test, expected_failure(abort_code = display_registry::ECapAlreadyClaimed), allow(dead_code)]
fun test_claim_cap_twice() {
    test_tx!(|registry, scenario| {
        let mut publisher = package::test_claim(MY_OTW {}, scenario.ctx());
        let cap = take_migration_cap(scenario);
        registry.migrate<MyKeyOnlyType>(&cap, vec_map::empty(), scenario.ctx());
        scenario.next_tx(@0x1);

        let mut display = scenario.take_shared<NewDisplay<MyKeyOnlyType>>();

        let _first = display.claim_with_publisher(&mut publisher, scenario.ctx());
        let _second = display.claim_with_publisher(&mut publisher, scenario.ctx());

        abort
    });
}

#[test, expected_failure(abort_code = display_registry::ECapNotClaimed), allow(dead_code)]
fun test_delete_legacy_before_migration() {
    test_tx!(|registry, scenario| {
        let cap = take_migration_cap(scenario);
        registry.migrate<MyKeyOnlyType>(&cap, vec_map::empty(), scenario.ctx());
        scenario.next_tx(@0x1);

        let display = scenario.take_shared<NewDisplay<MyKeyOnlyType>>();
        let publisher = package::test_claim(MY_OTW {}, scenario.ctx());
        let legacy_display = display::new<MyKeyOnlyType>(&publisher, scenario.ctx());
        display.delete_legacy(legacy_display);
        abort
    });
}

#[test, expected_failure(abort_code = display_registry::EFieldDoesNotExist), allow(dead_code)]
fun test_remove_non_existing_field() {
    test_tx!(|registry, scenario| {
        let cap = registry.new<MyKeyOnlyType>(vec_map::empty(), scenario.ctx());
        scenario.next_tx(@0x1);

        let mut display = scenario.take_shared<NewDisplay<MyKeyOnlyType>>();
        display.remove(&cap, DEMO_NAME_KEY.to_string());
        abort
    });
}

#[test, expected_failure(abort_code = display_registry::EFieldAlreadyExists), allow(dead_code)]
fun test_add_duplicate_field() {
    test_tx!(|registry, scenario| {
        let cap = registry.new<MyKeyOnlyType>(vec_map::empty(), scenario.ctx());
        scenario.next_tx(@0x1);

        let mut display = scenario.take_shared<NewDisplay<MyKeyOnlyType>>();
        display.add(&cap, DEMO_NAME_KEY.to_string(), DEMO_NAME_VALUE.to_string());
        display.add(&cap, DEMO_NAME_KEY.to_string(), b"".to_string());
        abort
    });
}

fun take_migration_cap(scenario: &mut Scenario): SystemMigrationCap {
    scenario.next_tx(display_registry::migration_cap_receiver());
    scenario.take_from_address<SystemMigrationCap>(display_registry::migration_cap_receiver())
}

/// Scaffold a test transaction, that produces a `Scenario` and a `DisplayRegistry` object.
macro fun test_tx($f: |&mut DisplayRegistry, &mut Scenario|) {
    let sender = @0x0;
    let mut scenario = test_scenario::begin(sender);

    display_registry::create_internal(scenario.ctx());
    scenario.next_tx(sender);

    let mut registry = scenario.take_shared<DisplayRegistry>();

    $f(&mut registry, &mut scenario);

    test_scenario::return_shared(registry);

    scenario.end();
}
