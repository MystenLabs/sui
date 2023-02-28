// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

module display_test::boars {
    use sui::object::{Self, UID};
    use std::option::{Self, Option};
    use sui::tx_context::{TxContext, sender};
    use sui::transfer::transfer;
    use sui::publisher;
    use sui::url::{Self, Url};
    use sui::display;
    use std::string::{utf8, String};

    /// For when a witness type passed is not an OTW.
    const ENotOneTimeWitness: u64 = 0;

    /// An OTW to use when creating a Publisher
    struct BOARS has drop {}

    struct Boar has key, store {
        id: UID,
        img_url: String,
        name: String,
        description: String,
        creator: Option<String>,
        price: Option<String>,
        metadata: Metadata,
        buyer: address,
        full_url: Url,
    }

    struct Metadata has store {
        age: u64,
    }

    fun init(otw: BOARS, ctx: &mut TxContext) {
        assert!(sui::types::is_one_time_witness(&otw), ENotOneTimeWitness);

        let pub = publisher::claim(otw, ctx);
        let display = display::new<Boar>(&pub, ctx);

        display::add_multiple(&mut display, vector[
            utf8(b"name"),
            utf8(b"description"),
            utf8(b"img_url"),
            utf8(b"creator"),
            utf8(b"price"),
            utf8(b"project_url"),
            utf8(b"age"),
            utf8(b"buyer"),
            utf8(b"full_url"),
            utf8(b"escape_syntax"),
        ], vector[
            utf8(b"{name}"),
            // test multiple fields and UID
            utf8(b"Unique Boar from the Boars collection with {name} and {id}"),
            utf8(b"https://get-a-boar.com/{img_url}"),
            // test option::some
            utf8(b"{creator}"),
            // test option::none
            utf8(b"{price}"),
            // test no template value
            utf8(b"https://get-a-boar.com/"),
            // test nested field
            utf8(b"{metadata.age}"),
            // test address
            utf8(b"{buyer}"),
            // test Url type
            utf8(b"{full_url}"),
            // test escape syntax
            utf8(b"\\{name\\}"),
        ]);

        transfer(display, sender(ctx));
        transfer(pub, sender(ctx));

        let boar = Boar {
            id: object::new(ctx),
            img_url: utf8(b"first.png"),
            name: utf8(b"First Boar"),
            description: utf8(b"First Boar from the Boars collection!"),
            creator: option::some(utf8(b"Chris")),
            price: option::none(),
            metadata: Metadata {
                age: 10,
            },
            buyer: sender(ctx),
            full_url: url::new_unsafe_from_bytes(b"https://get-a-boar.fullurl.com/"),
        };
        transfer(boar, sender(ctx))
    }
}
