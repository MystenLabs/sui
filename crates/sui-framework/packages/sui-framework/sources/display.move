// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

/// Defines a Display struct which describes the way an Object should be
/// displayed. The goal of the Sui Object Display is separation of the type
/// definition from its external (off-chain) representation.
///
/// Each of the fields of the Display object allows for pattern substitution and
/// filling-in the pieces using the data from the object T.
///
/// Example usage:
/// ```
/// module pkg::display;
///
/// use pkg::my_object::MyObject;
/// use sui::package;
/// use sui::display;
///
/// /// One time witness to get the `Publisher`.
/// public struct DISPLAY has drop {}
///
/// fun init(otw: DISPLAY, ctx: &mut TxContext) {
///   let publisher = package::claim(otw, ctx);
///   let display = display::new<MyObject>(&publisher, ctx);
///
///   display.add(b"name".to_string(), b"My Awesome Object".to_string());
///   display.add(b"description".to_string(), b");
///   display.commit(); // important to commit results on chain
/// }
/// ```
module sui::display;

use std::string::String;
use sui::event;
use sui::package::Publisher;
use sui::vec_map::{Self, VecMap};

/// For when `T` does not belong to the package `Publisher`.
const ENotOwner: u64 = 0;

/// The Display<T> object. Defines the way a T instance should be displayed
/// Display object can only be created and modified with the `Publisher` object,
/// making sure that the rules are set by the publisher of the type.
///
/// ```
/// // Example of a display object
/// Display<0x...::capy::Capy> {
///  fields:
///    name: "Capy { genes }"
///    link: "https://capy.art/capy/{ id }"
///    image_url: "https://api.capy.art/capy/{ id }/svg"
///    thumbnail_url: "https://api.capy.art/capy/{ id }/thumbnail"
///    description: "Lovely Capy, one of many"
///    project_url: "https://capy.art/"
///    creator: "Capy Lover"
/// }
/// ```
///
/// Uses only String type due to external-facing nature of the object, the
/// property names have priority over their types.
public struct Display<phantom T: key> has key, store {
    id: UID,
    /// Contains fields for display. Currently supported
    /// fields are: name, link, image and description.
    fields: VecMap<String, String>,
    /// Version that can only be updated manually by the publisher.
    version: u16,
}

/// Event: emitted when a new Display object has been created for type T.
/// Type signature of the event corresponds to the type while id serves for
/// the discovery.
///
/// Since Sui RPC supports querying events by type, finding a Display for the T
/// would be as simple as looking for the first event with `Display<T>`.
public struct DisplayCreated<phantom T: key> has copy, drop {
    id: ID,
}

/// Version of Display got updated via `commit` or `update_version`.
public struct VersionUpdated<phantom T: key> has copy, drop {
    id: ID,
    version: u16,
    fields: VecMap<String, String>,
}

// === Initializer Methods ===

/// Create an empty Display object. It can either be shared empty or filled
/// with data right away via cheaper `set_owned` method.
public fun new<T: key>(pub: &Publisher, ctx: &mut TxContext): Display<T> {
    assert!(is_authorized<T>(pub), ENotOwner);

    let id = object::new(ctx);
    event::emit(DisplayCreated<T> { id: id.to_inner() });
    Display { id, fields: vec_map::empty(), version: 0 }
}

/// Create a new Display<T> object with a set of fields.
public fun new_with_fields<T: key>(
    pub: &Publisher,
    fields: vector<String>,
    values: vector<String>,
    ctx: &mut TxContext,
): Display<T> {
    let mut display = new<T>(pub, ctx);
    fields.zip_do!(values, |name, value| display.fields.insert(name, value));
    display
}

// === Entry functions: Create ===

#[allow(lint(self_transfer))]
/// Create a new empty Display<T> object and keep it.
public entry fun create_and_keep<T: key>(pub: &Publisher, ctx: &mut TxContext) {
    transfer::public_transfer(new<T>(pub, ctx), ctx.sender())
}

/// Alias that matches the purpose better.
public use fun update_version as Display.commit;

/// Manually bump the version and emit an event with the updated version's contents.
public entry fun update_version<T: key>(display: &mut Display<T>) {
    display.version = display.version + 1;
    event::emit(VersionUpdated<T> {
        version: display.version,
        fields: *&display.fields,
        id: display.id.to_inner(),
    })
}

// === Entry functions: Add/Modify fields ===

/// Sets a custom `name` field with the `value`.
public entry fun add<T: key>(display: &mut Display<T>, name: String, value: String) {
    display.fields.insert(name, value)
}

/// Sets multiple `fields` with `values`.
public entry fun add_multiple<T: key>(
    display: &mut Display<T>,
    fields: vector<String>,
    values: vector<String>,
) {
    fields.zip_do!(values, |name, value| display.fields.insert(name, value));
}

/// Change the value of the field.
/// TODO (long run): version changes;
public entry fun edit<T: key>(display: &mut Display<T>, name: String, value: String) {
    let (_, _) = display.fields.remove(&name);
    display.fields.insert(name, value)
}

/// Remove the key from the Display.
public entry fun remove<T: key>(display: &mut Display<T>, name: String) {
    display.fields.remove(&name);
}

// === Access fields ===

/// Authorization check; can be performed externally to implement protection rules for Display.
public fun is_authorized<T: key>(pub: &Publisher): bool {
    pub.from_package<T>()
}

/// Read the `version` field.
public fun version<T: key>(d: &Display<T>): u16 {
    d.version
}

/// Read the `fields` field.
public fun fields<T: key>(d: &Display<T>): &VecMap<String, String> {
    &d.fields
}
