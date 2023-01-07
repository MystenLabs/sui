// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

/// Defines a Display struct which defines the way an Object
/// should be displayed. The intention is to keep data as independent
/// from its display as possible, protecting the development process
/// and keeping it separate from the ecosystem agreements.
///
/// Each of the fields of the Display object should allow for pattern
/// substitution and filling-in the pieces using the data from the object T.
module sui::display {
    use sui::publisher::{is_module, is_package, Publisher};
    use sui::tx_context::{TxContext, sender};
    use std::option::{Self, some, none};
    use std::string::{Self, utf8};
    use sui::object::{Self, UID};
    use sui::event::emit;
    use sui::transfer;

    /// The display standard. Defines the way an object should be
    /// displayed. New Display objects can only be created with a
    /// PublisherCap, making sure that the rules are set by the owner
    /// of the type.
    ///
    /// Each of the display properties should support patterns outside
    /// of the system, making it simpler to customize Display based
    /// on the property values of an Asset.
    ///
    /// Uses only String type for now; some fields may be replaced with
    /// Url's but due to external-facing nature of the object, the
    /// property names have a priority over types.
    struct Display<phantom T: key> has key {
        id: UID,
        name: Option<String>,
        link: Option<String>,
        image: Option<String>,
        description: Option<String>,

        // to be extended ?
        // possibly allow any dynamic fields to be added here
    }

    /// Event: emitted when a new Display object has been created for type T.
    /// Type signature of the event corresponds to the type while id serves for
    /// the discovery.
    struct DisplayCreated<phantom T: key> has copy, drop {
        id: ID
    }

    /// Set a name for the display.
    /// Eg: `My lovely capy {{genes}}` (for Capy project).
    entry public fun set_name<T: key>(d: &mut Display<T>, name: String) {
        d.name = some(name)
    }

    /// Set a link.
    entry public fun set_link<T: key>(d: &mut Display<T>, link: String) {
        d.link = some(link)
    }

    /// Set a link to an image
    entry public fun set_image<T: key>(d: &mut Display<T>, image: String) {
        d.image = some(image)
    }

    /// Set a description for the object.
    entry public fun set_description<T: key>(d: &mut Display<T>, desc: String) {
        d.desc = some(desc)
    }

    /// Create an empty Display object. It can either be
    /// shared empty of filled with data later on.
    public fun empty<T: key>(pub: &Publisher, ctx: &mut TxContext): Display<T> {
        let uid = object::new(ctx);

        event::emit(DisplayCreated {
            id: object::uid_to_inner(&uid)
        });

        Display {
            id: uid,
            name: none(),
            link: none(),
            image: none(),
            description: none(),
        }
    }

    /// Share an object. If the object was initially created
    /// empty and its values were set later.
    public fun share<T>(d: Display<T>) {
        transfer::share_object(d);
    }
}

#[test_only]
// module sui::display_capy {
//     use sui::object::{Self, UID};
//     use sui::publisher;
//     use sui::test_scenario as test;

//     struct Capy has key { id: UID }
//     struct CAPY has drop {}

//     #[test]
//     fun capy_init() {
//         // let test = test::begin(@0x2);
//         // let pub = publisher::test_claim(CAPY {}, test::ctx(&mut test));

//         // let display =
//     }
// }
