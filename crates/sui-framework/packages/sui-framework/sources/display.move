// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

/// Defines a Display struct which defines the way an Object
/// should be displayed. The intention is to keep data as independent
/// from its display as possible, protecting the development process
/// and keeping it separate from the ecosystem agreements.
///
/// Each of the fields of the Display object should allow for pattern
/// substitution and filling-in the pieces using the data from the object T.
///
/// More entry functions might be added in the future depending on the use cases.
#[deprecated(note = b"Use `sui::display_registry` instead")]
module sui::display;

use std::string::String;
use sui::package::Publisher;
use sui::vec_map::VecMap;

/// The Display<T> object. Defines the way a T instance should be
/// displayed. Display object can only be created and modified with
/// a PublisherCap, making sure that the rules are set by the owner
/// of the type.
///
/// Each of the display properties should support patterns outside
/// of the system, making it simpler to customize Display based
/// on the property values of an Object.
/// ```
/// // Example of a display object
/// Display<0x...::capy::Capy> {
///  fields:
///    <name, "Capy { genes }">
///    <link, "https://capy.art/capy/{ id }">
///    <image, "https://api.capy.art/capy/{ id }/svg">
///    <description, "Lovely Capy, one of many">
/// }
/// ```
///
/// Uses only String type due to external-facing nature of the object,
/// the property names have a priority over their types.
public struct Display<phantom T: key> has key, store {
    id: UID,
    /// Contains fields for display. Currently supported
    /// fields are: name, link, image and description.
    fields: VecMap<String, String>,
    /// Version that can only be updated manually by the Publisher.
    version: u16,
}

#[allow(unused_field)]
public struct DisplayCreated<phantom T: key> has copy, drop {
    id: ID,
}

#[allow(unused_field)]
public struct VersionUpdated<phantom T: key> has copy, drop {
    _id: ID,
    _version: u16,
    _fields: VecMap<String, String>,
}

public fun fields<T: key>(d: &Display<T>): &VecMap<String, String> { &d.fields }

public fun destroy<T: key>(display: Display<T>): VecMap<String, String> {
    let Display { id, fields, .. } = display;
    id.delete();
    fields
}

public(package) fun create_internal<T: key>(
    fields: VecMap<String, String>,
    ctx: &mut TxContext,
): Display<T> {
    Display { id: object::new(ctx), fields, version: 0 }
}

// === Deprecated ===

public fun new<T: key>(_: &Publisher, _: &mut TxContext): Display<T> { abort 1337 }

public fun new_with_fields<T: key>(
    _: &Publisher,
    _: vector<String>,
    _: vector<String>,
    _: &mut TxContext,
): Display<T> { abort 1337 }

#[allow(unused_type_parameter)]
public entry fun create_and_keep<T: key>(_: &Publisher, _: &mut TxContext) { abort 1337 }

public entry fun update_version<T: key>(_: &mut Display<T>) { abort 1337 }

public entry fun add<T: key>(_: &mut Display<T>, _: String, _: String) { abort 1337 }

public entry fun add_multiple<T: key>(_: &mut Display<T>, _: vector<String>, _: vector<String>) {
    abort 1337
}

public entry fun edit<T: key>(_: &mut Display<T>, _: String, _: String) { abort 1337 }

#[allow(unused_type_parameter)]
public entry fun remove<T: key>(_: &mut Display<T>, _: String) { abort 1337 }

#[allow(unused_type_parameter)]
public fun is_authorized<T: key>(_: &Publisher): bool { abort 1337 }

public fun version<T: key>(_: &Display<T>): u16 { abort 1337 }
