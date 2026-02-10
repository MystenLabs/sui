// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

#[test_only]
module sui::display_registry_tests;

use std::unit_test::assert_eq;
use sui::display::{Self, new};
use sui::display_registry::{Self, DisplayRegistry, Display, SystemMigrationCap, DisplayCap};
use sui::package::{Self, test_claim, Publisher};
use sui::test_scenario::{Self, Scenario};

public struct MyKeyOnlyType has key { id: UID }
public struct MyPotato {}
public struct MY_OTW has drop {}

const DEMO_NAME_KEY: vector<u8> = b"name";
const DEMO_NAME_VALUE: vector<u8> = b"{name}";

#[test]
fun create_modern_and_do_operations_e2e() {
    test_tx!(|registry, scenario| {
        let cap = new_display<MyPotato>(registry, scenario);
        scenario.next_tx(@0x1);
        let mut display = scenario.take_shared<Display<MyPotato>>();
        assert_eq!(display.fields().length(), 0);

        assert!(display.cap_id().is_some_and!(|cap_id| cap_id == object::id(&cap)));

        display.set(&cap, DEMO_NAME_KEY.to_string(), DEMO_NAME_VALUE.to_string());
        test_scenario::return_shared(display);

        scenario.next_tx(@0x1);
        let display = scenario.take_shared<Display<MyPotato>>();
        assert_eq!(display.fields().length(), 1);
        assert_eq!(*display.fields().get(&DEMO_NAME_KEY.to_string()), DEMO_NAME_VALUE.to_string());
        test_scenario::return_shared(display);

        scenario.next_tx(@0x1);
        let mut display = scenario.take_shared<Display<MyPotato>>();
        display.unset(&cap, DEMO_NAME_KEY.to_string());
        test_scenario::return_shared(display);
        scenario.next_tx(@0x1);

        let mut display = scenario.take_shared<Display<MyPotato>>();
        assert_eq!(display.fields().length(), 0);

        display.set(&cap, DEMO_NAME_KEY.to_string(), DEMO_NAME_VALUE.to_string());
        assert_eq!(display.fields().length(), 1);

        display.clear(&cap);
        assert_eq!(display.fields().length(), 0);

        test_scenario::return_shared(display);

        transfer::public_transfer(cap, @0x1);
    });
}

#[test]
fun claim_legacy() {
    test_tx!(|registry, scenario| {
        let publisher = new_publisher(scenario);
        let legacy_display = display::new<MyKeyOnlyType>(&publisher, scenario.ctx());
        // create a second legacy display so we can destroy it after claiming.
        let another_legacy_display = display::new<MyKeyOnlyType>(&publisher, scenario.ctx());

        let cap = take_migration_cap(scenario);

        // manually migrate `MyKeyOnlyType` to the new registry, as if we were the system.
        registry.system_migration<MyKeyOnlyType>(
            &cap,
            vector[],
            vector[],
            scenario.ctx(),
        );
        scenario.next_tx(@0x1);

        // Claim the display using our legacy display obj.
        let mut display = scenario.take_shared<Display<MyKeyOnlyType>>();
        assert!(display.cap_id().is_none());
        let new_cap = display.claim(legacy_display, scenario.ctx());

        // use the cap to edit display!
        display.set(&new_cap, DEMO_NAME_KEY.to_string(), DEMO_NAME_VALUE.to_string());

        // After claiming, we can delete all other legacy displays permissionless-ly.
        display.delete_legacy(another_legacy_display);
        test_scenario::return_shared(display);
        transfer::public_transfer(new_cap, @0x1);

        cap.destroy_system_migration_cap();
        publisher.burn();
    });
}

#[test]
fun migrate_legacy() {
    test_tx!(|registry, scenario| {
        let publisher = new_publisher(scenario);
        let mut legacy_display = display::new<MyKeyOnlyType>(&publisher, scenario.ctx());

        legacy_display.add(DEMO_NAME_KEY.to_string(), DEMO_NAME_VALUE.to_string());

        // Migrate Display using v1 to v2 successfully
        let (display, cap) = registry.migrate_v1_to_v2(legacy_display, scenario.ctx());

        assert!(display.cap_id().is_some_and!(|cap_id| cap_id == object::id(&cap)));
        assert_eq!(display.fields().length(), 1);
        assert_eq!(*display.fields().get(&DEMO_NAME_KEY.to_string()), DEMO_NAME_VALUE.to_string());

        display.share();
        transfer::public_transfer(cap, @0x1);

        publisher.burn();
    });
}

#[test]
fun claim_with_publisher() {
    test_tx!(|registry, scenario| {
        let mut publisher = package::test_claim(MY_OTW {}, scenario.ctx());
        let cap = take_migration_cap(scenario);
        registry.system_migration<MyKeyOnlyType>(
            &cap,
            vector[],
            vector[],
            scenario.ctx(),
        );
        scenario.next_tx(@0x1);

        let mut display = scenario.take_shared<Display<MyKeyOnlyType>>();
        assert!(display.cap_id().is_none());
        let new_cap = display.claim_with_publisher(&mut publisher, scenario.ctx());
        display.set(&new_cap, DEMO_NAME_KEY.to_string(), DEMO_NAME_VALUE.to_string());
        test_scenario::return_shared(display);
        transfer::public_transfer(new_cap, @0x1);

        cap.destroy_system_migration_cap();
        publisher.burn();
    });
}

#[test]
fun update_field() {
    test_tx!(|registry, scenario| {
        let cap = new_display<MyKeyOnlyType>(registry, scenario);
        scenario.next_tx(@0x1);

        let mut display = scenario.take_shared<Display<MyKeyOnlyType>>();
        // Add `field` to display.
        display.set(&cap, DEMO_NAME_KEY.to_string(), DEMO_NAME_VALUE.to_string());
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
fun create_display_twice_fails() {
    test_tx!(|registry, scenario| {
        let pub = new_publisher(scenario);
        let (_display, _cap) = registry.new_with_publisher<MyKeyOnlyType>(
            &pub,
            scenario.ctx(),
        );
        let (__display, __cap) = registry.new_with_publisher<MyKeyOnlyType>(
            &pub,
            scenario.ctx(),
        );
        abort
    });
}

#[test]
fun migrate_twice_returns_silently() {
    test_tx!(|registry, scenario| {
        scenario.next_tx(@0x5);
        let cap = take_migration_cap(scenario);
        registry.system_migration<MyKeyOnlyType>(
            &cap,
            vector[DEMO_NAME_KEY.to_string()],
            vector[DEMO_NAME_VALUE.to_string()],
            scenario.ctx(),
        );
        let effects = scenario.next_tx(@0x5);

        // we created Display for `MyKeyOnlyType`
        assert_eq!(effects.shared().length(), 1);
        assert_eq!(effects.created().length(), 1);

        // try to migrate again, should have no object creations.
        registry.system_migration<MyKeyOnlyType>(
            &cap,
            vector[],
            vector[],
            scenario.ctx(),
        );
        let effects = scenario.next_tx(@0x5);

        assert_eq!(effects.shared().length(), 0);
        assert_eq!(effects.created().length(), 0);

        // We should have the state of the first migration.
        let display = scenario.take_shared<Display<MyKeyOnlyType>>();
        assert_eq!(display.fields().length(), 1);
        assert_eq!(*display.fields().get(&DEMO_NAME_KEY.to_string()), DEMO_NAME_VALUE.to_string());

        test_scenario::return_shared(display);

        cap.destroy_system_migration_cap();
    });
}

#[test, expected_failure(abort_code = display_registry::ECapAlreadyClaimed), allow(dead_code)]
fun claim_cap_twice_fails() {
    test_tx!(|registry, scenario| {
        let mut publisher = package::test_claim(MY_OTW {}, scenario.ctx());
        let cap = take_migration_cap(scenario);
        registry.system_migration<MyKeyOnlyType>(
            &cap,
            vector[],
            vector[],
            scenario.ctx(),
        );
        scenario.next_tx(@0x1);

        let mut display = scenario.take_shared<Display<MyKeyOnlyType>>();

        let _first = display.claim_with_publisher(&mut publisher, scenario.ctx());
        let _second = display.claim_with_publisher(&mut publisher, scenario.ctx());

        abort
    });
}

#[test, expected_failure(abort_code = display_registry::ECapNotClaimed), allow(dead_code)]
fun delete_legacy_before_migration_fails() {
    test_tx!(|registry, scenario| {
        let cap = take_migration_cap(scenario);
        registry.system_migration<MyKeyOnlyType>(
            &cap,
            vector[],
            vector[],
            scenario.ctx(),
        );
        scenario.next_tx(@0x1);

        let display = scenario.take_shared<Display<MyKeyOnlyType>>();
        let publisher = package::test_claim(MY_OTW {}, scenario.ctx());
        let legacy_display = display::new<MyKeyOnlyType>(&publisher, scenario.ctx());
        display.delete_legacy(legacy_display);
        abort
    });
}

#[test, expected_failure(abort_code = display_registry::EFieldDoesNotExist), allow(dead_code)]
fun remove_non_existing_field_fails() {
    test_tx!(|registry, scenario| {
        let cap = new_display<MyKeyOnlyType>(registry, scenario);
        scenario.next_tx(@0x1);

        let mut display = scenario.take_shared<Display<MyKeyOnlyType>>();
        display.unset(&cap, DEMO_NAME_KEY.to_string());
        abort
    });
}

#[test, expected_failure(abort_code = display_registry::ENotValidPublisher), allow(dead_code)]
fun use_invalid_publisher() {
    test_tx!(|registry, scenario| {
        // Try claim display for `std` (external package) using `sui`'s publisher.
        let _cap = new_display<std::string::String>(registry, scenario);
        abort
    });
}

#[test, expected_failure(abort_code = display_registry::EDisplayAlreadyExists), allow(dead_code)]
fun fails_when_migrating_twice() {
    test_tx!(|registry, scenario| {
        let publisher = new_publisher(scenario);
        let legacy_display = display::new<MyKeyOnlyType>(&publisher, scenario.ctx());
        let another_legacy_display = display::new<MyKeyOnlyType>(&publisher, scenario.ctx());

        // Migrate Display using v1 to v2 successfully
        let (_display, _cap) = registry.migrate_v1_to_v2(legacy_display, scenario.ctx());
        let (_another_display, _another_cap) = registry.migrate_v1_to_v2(
            another_legacy_display,
            scenario.ctx(),
        );

        abort
    });
}

fun new_display<T>(registry: &mut DisplayRegistry, scenario: &mut Scenario): DisplayCap<T> {
    let publisher = new_publisher(scenario);
    let (display, cap) = registry.new_with_publisher<T>(&publisher, scenario.ctx());

    publisher.burn();
    display.share();
    cap
}

fun new_publisher(scenario: &mut Scenario): Publisher {
    package::test_claim(MY_OTW {}, scenario.ctx())
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
