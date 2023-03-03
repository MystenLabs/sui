// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

#[test_only]
/// Initializes a simple collection.
module sui::boars {
    use sui::tx_context::{TxContext, sender};
    use sui::transfer::transfer;
    use sui::collectible;
    use sui::display;
    use std::string::utf8;
    use std::option;

    /// Limit how many objects can be created within this collection.
    const CAP: u64 = 1000;

    /// An OTW to use when creating a Publisher and CollectionCreatorCap
    struct BOARS has drop {}

    /// The type of the Collectible; used only as a phantom parameter, and
    /// does not require abilities. The type use is authorized with the Publisher
    /// object and is hard-capped to the module (rather than the package).
    struct Boar has store {}

    fun init(otw: BOARS, ctx: &mut TxContext) {
        let (pub, display, creator_cap) = collectible::create_collection<BOARS, Boar>(
            otw, option::some(CAP), ctx
        );

        display::add_multiple(&mut display, vector[
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

        display::update_version(&mut display);
        transfer(creator_cap, sender(ctx));
        transfer(display, sender(ctx));
        transfer(pub, sender(ctx))
    }

    #[test_only]
    public fun init_for_testing(ctx: &mut TxContext) {
        init(BOARS {}, ctx)
    }
}

#[test_only]
module sui::collectible_tests {
    use sui::test_scenario::{Self as ts};
    use sui::collectible::{Self, CollectionCreatorCap};
    use sui::boars::{Self, Boar};
    use sui::transfer::transfer;
    use std::option::{some, none};
    use std::string::utf8;
    use std::vector as vec;

    public fun boys(): (address, address) { (@0xBAD, @0xDEAD) }

    #[test]
    fun init_and_mint_boars() {
        let (creator, _) = boys();

        let test = ts::begin(creator);
        ts::next_tx(&mut test, creator); {
            boars::init_for_testing(ts::ctx(&mut test));
        };

        ts::next_tx(&mut test, creator); {
            let creator_cap = ts::take_from_address<CollectionCreatorCap<Boar>>(&test, creator);
            let boar = collectible::mint(
                &mut creator_cap,
                utf8(b"boar_1.jpg"),
                some(utf8(b"Boario")),
                none(),
                none(),
                none(),
                ts::ctx(&mut test)
            );

            transfer(creator_cap, creator);
            transfer(boar, creator)
        };

        ts::next_tx(&mut test, creator); {
            let creator_cap = ts::take_from_address<CollectionCreatorCap<Boar>>(&test, creator);
            let boars = collectible::batch_mint(
                &mut creator_cap,
                vector[ utf8(b"boar_2.jpg"), utf8(b"boar_3.jpg"), utf8(b"boar_4.jpg") ],
                some(vector[ utf8(b"Buddy Boar"), utf8(b"Boarskin"), utf8(b"Boarson") ]),
                none(),
                none(),
                none(),
                ts::ctx(&mut test)
            );

            while (vec::length(&boars) > 0) {
                transfer(vec::pop_back(&mut boars), creator);
            };

            transfer(creator_cap, creator);
            vec::destroy_empty(boars);
        };

        ts::end(test);
    }
}
