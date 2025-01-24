// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

//# init --protocol-version 51 --addresses Test=0x0 --accounts A --simulator

//# publish --sender A
module Test::boars {
    use sui::package;
    use sui::url::{Self, Url};
    use sui::display;
    use std::string::{utf8, String};

    /// For when a witness type passed is not an OTW.
    const ENotOneTimeWitness: u64 = 0;

    /// An OTW to use when creating a Publisher
    public struct BOARS has drop {}

    public struct Boar has key, store {
        id: UID,
        img_url: String,
        name: String,
        description: String,
        creator: Option<String>,
        price: Option<String>,
        metadata: NestedMetadata,
        full_url: Url,
        nums: u64,
        bools: bool,
        buyer: address,
        vec: vector<u64>,
    }

    public struct Metadata has store {
        age: u64,
    }

    public struct NestedMetadata has store {
        nested: Metadata
    }

    fun init(otw: BOARS, ctx: &mut TxContext) {
        let pub = package::claim(otw, ctx);
        let display = display::new<Boar>(&pub, ctx);

        transfer::public_transfer(display, ctx.sender());
        transfer::public_transfer(pub, ctx.sender());
    }


    public entry fun update_display_faulty(display_obj: &mut display::Display<Boar>) {
      display::add_multiple(display_obj, vector[
        utf8(b"vectors"),
        utf8(b"idd"),
        utf8(b"namee"),
      ], vector[
        utf8(b"{vec}"),
        utf8(b"{idd}"),
        utf8(b"{namee}"),
      ]);
      display::update_version(display_obj)
    }

    public entry fun single_add(display_obj: &mut display::Display<Boar>) {
      display::add(display_obj, utf8(b"nums"), utf8(b"{nums}"));
      display::update_version(display_obj)
    }

    public entry fun multi_add(display_obj: &mut display::Display<Boar>) {
        display::add_multiple(display_obj, vector[
            utf8(b"bools"),
            utf8(b"buyer"),
            utf8(b"name"),
            utf8(b"creator"),
            utf8(b"price"),
            utf8(b"project_url"),
            utf8(b"base_url"),
            utf8(b"no_template"),
            utf8(b"age"),
            utf8(b"full_url"),
            utf8(b"escape_syntax"),
        ], vector[
            // test bool
            utf8(b"{bools}"),
            // test address
            utf8(b"{buyer}"),
            // test string
            utf8(b"{name}"),
            // test optional string w/ Some value
            utf8(b"{creator}"),
            // test optional string w/ None value
            utf8(b"{price}"),
            // test multiple fields and UID
            utf8(b"Unique Boar from the Boars collection with {name} and {id}"),
            utf8(b"https://get-a-boar.com/{img_url}"),
            // test no template value
            utf8(b"https://get-a-boar.com/"),
            // test nested struct
            utf8(b"{metadata.nested.age}"),
            // test Url type
            utf8(b"{full_url}"),
            // test escape syntax
            utf8(b"\\{name\\}"),
        ]);

        display::update_version(display_obj);
    }

    public entry fun create_bear(ctx: &mut TxContext) {
        let boar = Boar {
            id: object::new(ctx),
            img_url: utf8(b"first.png"),
            name: utf8(b"First Boar"),
            description: utf8(b"First Boar from the Boars collection!"),
            creator: option::some(utf8(b"Will")),
            price: option::none(),
            metadata: NestedMetadata {
                nested: Metadata {
                    age: 10,
                },
            },
            full_url: url::new_unsafe_from_bytes(b"https://get-a-boar.fullurl.com/"),
            nums: 420,
            bools: true,
            buyer: ctx.sender(),
            vec: vector[1, 2, 3],
        };
        transfer::transfer(boar, ctx.sender())
    }
}

//# create-checkpoint

//# view-checkpoint

//# run Test::boars::create_bear --sender A

//# run Test::boars::update_display_faulty --sender A --args object(1,1)

//# create-checkpoint

//# view-checkpoint

//# run-graphql
{
  address(address: "@{A}") {
    objects(filter: {type: "@{Test}::boars::Boar"}) {
      nodes {
        display {
          key
          value
          error
        }
      }
    }
  }
}

//# run Test::boars::single_add --sender A --args object(1,1)

//# create-checkpoint

//# view-checkpoint

//# run-graphql
{
  address(address: "@{A}") {
    objects(filter: {type: "@{Test}::boars::Boar"}) {
      nodes {
        display {
          key
          value
          error
        }
      }
    }
  }
}

//# run Test::boars::multi_add --sender A --args object(1,1)

//# create-checkpoint

//# view-checkpoint

//# run-graphql
{
  address(address: "@{A}") {
    objects(filter: {type: "@{Test}::boars::Boar"}) {
      nodes {
        display {
          key
          value
          error
        }
      }
    }
  }
}
