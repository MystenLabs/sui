// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

module sui::display_registry;

use std::string::String;
use sui::derived_object;
use sui::display::Display as LegacyDisplay;
use sui::package::Publisher;
use sui::vec_map::{Self, VecMap};

/// TODO: Fill this in with the programmatic address responsible for
/// migrating all V1 displays into V2.
const SYSTEM_MIGRATION_ADDRESS: address = @0xf00;

/// The version of the Display language that is currently supported.
const DISPLAY_VERSION_2: u16 = 2;

/// The current language version for Display. That helps parsers
/// decide how to process display fields.
const LANGUAGE_VERSION: u16 = DISPLAY_VERSION_2;

#[error(code = 0)]
const ENotSystemAddress: vector<u8> = b"This is only callable from system address.";
#[error(code = 1)]
const EDisplayAlreadyExists: vector<u8> = b"Display for the supplied type already exists.";
#[error(code = 2)]
const ECapAlreadyClaimed: vector<u8> = b"Cap for this display object has already been claimed.";
#[error(code = 3)]
const ENotValidPublisher: vector<u8> = b"The publisher is not valid for the supplied type.";
#[error(code = 4)]
const EFieldDoesNotExist: vector<u8> = b"Field does not exist in the display.";
#[error(code = 5)]
const ECapNotClaimed: vector<u8> =
    b"Cap for this display object has not been claimed so you cannot delete the legacy display yet.";

/// The root of display, to enable derivation of addresses.
/// We'll most likely deploy this into `0xd`
public struct DisplayRegistry has key { id: UID }

/// A singleton capability object to enable migrating all V1 displays into
/// V2. We don't wanna support indexing for legacy display objects,
/// so this will forcefully move all existing display instances to use the registry.
public struct SystemMigrationCap has key { id: UID }

/// This is the struct that holds the display values for a type T.
public struct Display<phantom T> has key {
    id: UID,
    /// All the (key,value) entries for a given display object.
    fields: VecMap<String, String>,
    /// The "template" version of display. This dictates the language
    /// that the display parser needs to use, and enables permissionless on-chain
    /// upgrades to the language.
    /// This is not related to the `legacy` version field, which is deprecated.
    language_version: u16,
    /// The capability object ID. It's `Option` because legacy Displays will need claiming.
    cap_id: Option<ID>,
}

/// The capability object that is used to manage the display.
public struct DisplayCap<phantom T> has key, store { id: UID }

/// The key used for deriving the instance of `Display`. Contains the version of
/// the Display language in it to separate concerns.
public struct DisplayKey<phantom T>(u16) has copy, drop, store;

/// Create a new Display object for a given type `T` using `internal::Permit` to
/// prove type ownership.
public fun new<T>(
    registry: &mut DisplayRegistry,
    _: internal::Permit<T>,
    ctx: &mut TxContext,
): (Display<T>, DisplayCap<T>) {
    let key = DisplayKey<T>(LANGUAGE_VERSION);
    assert!(!derived_object::exists(&registry.id, key), EDisplayAlreadyExists);
    let display = Display<T> {
        id: derived_object::claim(&mut registry.id, key),
        fields: vec_map::empty(),
        language_version: LANGUAGE_VERSION,
        cap_id: option::none(),
    };
    let cap = DisplayCap<T> { id: object::new(ctx) };
    (display, cap)
}

/// Create a new display object using the `Publisher` object.
public fun new_with_publisher<T>(
    registry: &mut DisplayRegistry,
    publisher: &Publisher,
    ctx: &mut TxContext,
): (Display<T>, DisplayCap<T>) {
    let key = DisplayKey<T>(LANGUAGE_VERSION);

    assert!(!derived_object::exists(&registry.id, key), EDisplayAlreadyExists);
    assert!(publisher.from_package<T>(), ENotValidPublisher);
    let cap = DisplayCap<T> { id: object::new(ctx) };
    let display = Display<T> {
        id: derived_object::claim(&mut registry.id, key),
        fields: vec_map::empty(),
        language_version: LANGUAGE_VERSION,
        cap_id: option::some(cap.id.to_inner()),
    };
    (display, cap)
}

/// Unset a key from display.
public fun unset<T>(display: &mut Display<T>, _: &DisplayCap<T>, name: String) {
    assert!(display.fields.contains(&name), EFieldDoesNotExist);
    display.fields.remove(&name);
}

/// Replace an existing key with the supplied one.
public fun set<T>(display: &mut Display<T>, _: &DisplayCap<T>, name: String, value: String) {
    if (display.fields.contains(&name)) {
        display.fields.remove(&name);
    };
    display.fields.insert(name, value);
}

/// Clear the display vec_map, allowing a fresh re-creation of fields
public fun clear<T>(display: &mut Display<T>, _: &DisplayCap<T>) {
    display.fields = vec_map::empty();
}

/// Share the `Display` object to finalize the creation.
public fun share<T>(display: Display<T>) {
    transfer::share_object(display)
}

/// Allow a legacy Display holder to claim the capability object.
public fun claim<T: key>(
    display: &mut Display<T>,
    legacy: LegacyDisplay<T>,
    ctx: &mut TxContext,
): DisplayCap<T> {
    assert!(display.cap_id.is_none(), ECapAlreadyClaimed);
    let cap = DisplayCap<T> { id: object::new(ctx) };
    display.cap_id = option::some(cap.id.to_inner());
    legacy.destroy();
    cap
}

/// Allow claiming a new display using `Publisher` as proof of ownership.
public fun claim_with_publisher<T: key>(
    display: &mut Display<T>,
    publisher: &mut Publisher,
    ctx: &mut TxContext,
): DisplayCap<T> {
    assert!(display.cap_id.is_none(), ECapAlreadyClaimed);
    assert!(publisher.from_package<T>(), ENotValidPublisher);
    let cap = DisplayCap<T> { id: object::new(ctx) };
    display.cap_id = option::some(cap.id.to_inner());
    cap
}

/// Allow the `SystemMigrationCap` holder to create display objects with supplied
/// values. The migration is performed once on launch of the DisplayRegistry,
/// further migrations will have to be performed for each object, and will only
/// be possible until legacy `display` methods are finally deprecated.
public fun migrate_v1_to_v2_with_system_migration_cap<T: key>(
    registry: &mut DisplayRegistry,
    _: &SystemMigrationCap,
    fields: VecMap<String, String>,
    _ctx: &mut TxContext,
) {
    // System migration is only possible for V1 to V2.
    // Should it keep V1 in Display originally?
    let key = DisplayKey<T>(DISPLAY_VERSION_2);

    assert!(!derived_object::exists(&registry.id, key), EDisplayAlreadyExists);

    transfer::share_object(Display<T> {
        id: derived_object::claim(&mut registry.id, key),
        fields,
        language_version: DISPLAY_VERSION_2,
        cap_id: option::none(),
    });
}

// TODO: decide on whether to keep this function or not.
public fun migrate_v1_to_v2<T: key>(_: LegacyDisplay<T>): (Display<T>, DisplayCap<T>) { abort }

/// Destroy the `SystemMigrationCap` after successfully migrating all V1 instances.
entry fun destroy_system_migration_cap(cap: SystemMigrationCap) {
    let SystemMigrationCap { id } = cap;
    id.delete();
}

/// Allow deleting legacy display objects, as long as the cap has been claimed first.
public fun delete_legacy<T: key>(display: &Display<T>, legacy: LegacyDisplay<T>) {
    assert!(display.cap_id.is_some(), ECapNotClaimed);
    legacy.destroy();
}

/// Get a reference to the fields of display.
public fun fields<T>(display: &Display<T>): &VecMap<String, String> {
    &display.fields
}

public(package) fun create_internal(ctx: &mut TxContext) {
    assert!(ctx.sender() == @0x0, ENotSystemAddress);

    // TODO: Replace with known system address.
    transfer::share_object(DisplayRegistry { id: object::new(ctx) });
    transfer::transfer(SystemMigrationCap { id: object::new(ctx) }, SYSTEM_MIGRATION_ADDRESS);
}

public(package) fun migration_cap_receiver(): address {
    SYSTEM_MIGRATION_ADDRESS
}

// Create a new display registry object callable only from 0x0 (end of epoch)
#[allow(unused_function)]
fun create(ctx: &mut TxContext) {
    create_internal(ctx);
}
