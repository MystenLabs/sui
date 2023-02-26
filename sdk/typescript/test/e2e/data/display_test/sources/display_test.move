// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

module display_test::boars {
    use sui::object::{Self, UID};
    use sui::tx_context::{TxContext, sender};
    use sui::transfer::transfer;
    use sui::publisher;
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
        creator: String,
    }

    fun init(otw: BOARS, ctx: &mut TxContext) {
        assert!(sui::types::is_one_time_witness(&otw), ENotOneTimeWitness);

        let pub = publisher::claim(otw, ctx);
        let display = display::new<Boar>(&pub, ctx);

        display::add_multiple(&pub, &mut display, vector[
            utf8(b"name"),
            utf8(b"description"),
            utf8(b"img_url"),
            utf8(b"creator"),
            utf8(b"project_url")
        ], vector[
            utf8(b"{name}"),
            utf8(b"Unique Boar from the Boars collection!"),
            utf8(b"https://get-a-boar.com/{img_url}"),
            utf8(b"Boarcognito"),
            utf8(b"https://get-a-boar.com/")
        ]);

        display::update_version(&pub, &mut display);
        display::transfer(display, sender(ctx));
        transfer(pub, sender(ctx));

        let boar = Boar {
            id: object::new(ctx),
            img_url: utf8(b"first.png"),
            name: utf8(b"First Boar"),
            description: utf8(b"First Boar from the Boars collection!"),
            creator: utf8(b"Chris"),
        };
        transfer(boar, sender(ctx))
    }
}
