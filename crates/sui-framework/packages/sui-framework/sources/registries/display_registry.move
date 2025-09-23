// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

module sui::display_registry;

use std::string::String;
use sui::derived_object;
use sui::display::Display;
use sui::package::Publisher;
use sui::vec_map::VecMap;

/// TODO: Fill this in with the programmatic address responsible for
/// migrating all V1 displays into V2.
const SYSTEM_MIGRATION_ADDRESS: address = @0xf00;
/// The current language version for Display. That helps parsers
/// decide how to process display fields.
const LANGUAGE_VERSION: u16 = 0;

#[error(code = 0)]
const ENotSystemAddress: vector<u8> = b"This is only callable from system address.";
#[error(code = 1)]
const EDisplayAlreadyExists: vector<u8> = b"Display for the supplied type already exists.";
#[error(code = 2)]
const ECapAlreadyClaimed: vector<u8> = b"Cap for this display object has already been claimed.";
#[error(code = 3)]
const ENotValidPublisher: vector<u8> = b"The publisher is not valid for the supplied type.";
#[error(code = 4)]
const EFieldAlreadyExists: vector<u8> =
    b"Field already exists in the display. Call `update` instead.";
#[error(code = 5)]
const EFieldDoesNotExist: vector<u8> = b"Field does not exist in the display.";
#[error(code = 6)]
const ECapNotClaimed: vector<u8> =
    b"Cap for this display object has not been claimed so you cannot delete the legacy display yet.";

/// The root of display, to enable derivation of addresses.
/// We'll most likely deploy this into `0xd`
public struct DisplayRegistry has key {
    id: UID,
}

/// A singleton capability object to enable migrating all V1 displays into
/// V2. We don't wanna support indexing for legacy display objects,
/// so this will forcefully move all existing display instances to use the registry.
public struct SystemMigrationCap has key {
    id: UID,
}

/// TODO: Come up with a better name lol
///
/// This is the struct that holds the display values for a type T.
public struct NewDisplay<phantom T> has key {
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

/// The key used for deriving the instance of `NewDisplay`.
public struct DisplayKey<phantom T>() has copy, drop, store;

/// The capability object that is used to manage the display.
public struct DisplayCap<phantom T> has key, store {
    id: UID,
}

/// Create a new display object.
/// TODO: Add internal verifier rule that we can only create display for types we own in the package.
/// TODO(2): Should we allow `new_with_publisher`, to keep creations compatible?
public fun new<T /*internal*/>(
    registry: &mut DisplayRegistry,
    fields: VecMap<String, String>,
    ctx: &mut TxContext,
): DisplayCap<T> {
    assert!(!derived_object::exists(&registry.id, DisplayKey<T>()), EDisplayAlreadyExists);
    let cap = DisplayCap<T> { id: object::new(ctx) };
    let display = NewDisplay<T> {
        id: derived_object::claim(&mut registry.id, DisplayKey<T>()),
        fields,
        language_version: LANGUAGE_VERSION,
        cap_id: option::some(cap.id.to_inner()),
    };
    transfer::share_object(display);
    cap
}

/// Add a `key,value` to display.
public fun add<T>(display: &mut NewDisplay<T>, _: &DisplayCap<T>, name: String, value: String) {
    assert!(!display.fields.contains(&name), EFieldAlreadyExists);
    display.fields.insert(name, value);
}

/// Remove a key from display.
public fun remove<T>(display: &mut NewDisplay<T>, _: &DisplayCap<T>, name: String) {
    assert!(display.fields.contains(&name), EFieldDoesNotExist);
    display.fields.remove(&name);
}

/// Replace an existing key with the supplied one.
public fun update<T>(display: &mut NewDisplay<T>, _: &DisplayCap<T>, name: String, value: String) {
    if (display.fields.contains(&name)) {
        display.fields.remove(&name);
    };
    display.fields.insert(name, value);
}

/// Allow a legacy Display holder to claim the capability object.
public fun claim<T: key>(
    display: &mut NewDisplay<T>,
    legacy: Display<T>,
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
    display: &mut NewDisplay<T>,
    publisher: &mut Publisher,
    ctx: &mut TxContext,
): DisplayCap<T> {
    assert!(display.cap_id.is_none(), ECapAlreadyClaimed);
    assert!(publisher.from_package<T>(), ENotValidPublisher);
    let cap = DisplayCap<T> { id: object::new(ctx) };
    display.cap_id = option::some(cap.id.to_inner());
    cap
}

/// Allow the `SystemMigrationCap` holder to create display objects with supplied values.
public fun migrate<T: key>(
    registry: &mut DisplayRegistry,
    _: &SystemMigrationCap,
    fields: VecMap<String, String>,
    _ctx: &mut TxContext,
) {
    assert!(!derived_object::exists(&registry.id, DisplayKey<T>()), EDisplayAlreadyExists);
    // TODO: Should we transform fields on-chain or off-chain (for the new parsing style?)
    transfer::share_object(NewDisplay<T> {
        id: derived_object::claim(&mut registry.id, DisplayKey<T>()),
        fields,
        language_version: LANGUAGE_VERSION,
        cap_id: option::none(),
    });
}

/// Destroy the `SystemMigrationCap` after successfuly migrating all V1 instances.
entry fun destroy_cap(cap: SystemMigrationCap) {
    let SystemMigrationCap { id } = cap;
    id.delete();
}

/// Allow deleting legacy display objects, as long as the cap has been claimed first.
public fun delete_legacy<T: key>(display: &NewDisplay<T>, legacy: Display<T>) {
    assert!(display.cap_id.is_some(), ECapNotClaimed);
    legacy.destroy();
}

// Create a new display registry object callable only from 0x0 (end of epoch)
fun create(ctx: &mut TxContext) {
    assert!(ctx.sender() == @0x0, ENotSystemAddress);

    // TODO: Replace with known system address.
    transfer::share_object(DisplayRegistry { id: object::new(ctx) });

    transfer::transfer(SystemMigrationCap { id: object::new(ctx) }, SYSTEM_MIGRATION_ADDRESS);
}
