module sui::display_registry;

use std::string::String;
use sui::display::{Self, Display};
use sui::dynamic_field;
use sui::package::Publisher;
use sui::transfer::Receiving;
use sui::vec_map::VecMap;

const EUnauthorized: u64 = 0;
const EDisplayAlreadyExists: u64 = 1;
const ENotSystemAddress: u64 = 2;

public struct Registry has key { id: UID }
public struct DisplayKey<phantom T>() has copy, drop, store;
public struct DisplayCap<phantom T> has key, store { id: UID }
public struct DisplayV2(VecMap<String, String>) has copy, drop, store;

/// Sets the display for a given type.
///
/// This will overwrite the existing display for the given type,
/// making this the default for the given type.
public fun set_display<T>(
    _: &DisplayCap<T>,
    registry: &mut Registry,
    fields: VecMap<String, String>,
) {
    let fields = DisplayV2(fields);
    if (registry.exists<T>()) {
        dynamic_field::remove<_, DisplayV2>(&mut registry.id, DisplayKey<T>());
    };

    registry.set<T>(fields);
}

/// Allows the "publisher" to claim a Cap for editing Display for a given type.
public fun claim_cap<T>(publisher: &mut Publisher, ctx: &mut TxContext): DisplayCap<T> {
    assert!(publisher.from_package<T>(), EUnauthorized);
    DisplayCap { id: object::new(ctx) }
}

#[allow(deprecated_usage)]
public fun migrate<T: key>(registry: &mut Registry, v1: &Display<T>) {
    registry.migrate_if_not_exists<T>(v1);
}

#[allow(deprecated_usage)]
public fun migrate_receiving<T: key>(registry: &mut Registry, v1_receiving: Receiving<Display<T>>) {
    let v1 = transfer::public_receive(&mut registry.id, v1_receiving);
    registry.migrate_if_not_exists<T>(&v1);
    v1.destroy();
}

#[allow(deprecated_usage)]
/// This function is used to allow setting the Display for a type `T` in `init` function.
/// This will be removed once we have support for `objects` in `init` functions.
///
/// This can be called to create a Display object in the `init` function, and transfer it
/// to the registry.
///
/// Then, it can be permissionlessly picked up by calling the `migrate_receiving` function.
public fun create_legacy<T: key>(
    _: &DisplayCap<T>,
    fields: VecMap<String, String>,
    ctx: &mut TxContext,
) {
    let v1 = display::create_internal<T>(fields, ctx);
    // TODO: Replace this with the "known" ID for the registry.
    transfer::public_transfer(v1, @0x0);
}

#[allow(deprecated_usage)]
fun migrate_if_not_exists<T: key>(registry: &mut Registry, v1: &Display<T>) {
    assert!(!registry.exists<T>(), EDisplayAlreadyExists);
    registry.set<T>(DisplayV2(*v1.fields()))
}

fun set<T>(registry: &mut Registry, v2: DisplayV2) {
    assert!(!registry.exists<T>(), EDisplayAlreadyExists);
    dynamic_field::add(&mut registry.id, DisplayKey<T>(), v2);
}

fun exists<T>(registry: &Registry): bool {
    dynamic_field::exists_(&registry.id, DisplayKey<T>())
}

#[allow(unused_function)]
/// Create and share the singleton Registry -- this function is
fun create(ctx: &TxContext) {
    assert!(ctx.sender() == @0x0, ENotSystemAddress);

    transfer::share_object(Registry {
        id: object::sui_display_registry_object_id(),
    })
}
