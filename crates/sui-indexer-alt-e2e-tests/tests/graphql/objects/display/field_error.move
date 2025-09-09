// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

//# init --protocol-version 70 --accounts A --addresses test=0x0 --simulator

// 1. Publish a package that includes a Display format.
// 2. Create an object from this package
// 3. Try to view the contents of the new object -- this should fail because it
//    is too big to deserialize fully.
// 4. Try to view its display, which will work, but selectively.

//# publish --sender A
module test::mod {
  use std::string::utf8;
  use sui::display;
  use sui::package;

  public struct MOD() has drop;

  public struct Foo has key, store {
    id: UID,
    bar: Bar,
    baz: u64,
    qux: vector<u64>,
  }

  /// Bar is too big to deserialize, so attempting to display it in its
  /// entirety will fail, even though displaying something inside it would be
  /// fine.
  public struct Bar has store {
    chunky: vector<Chunky<Chunky<Chunky<Chunky<u8>>>>>,
    val: u64,
  }

  public struct Chunky<T: store> has store {
    long_field_name_0000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000: T,
  }

  fun init(otw: MOD, ctx: &mut TxContext) {
    let publisher = package::claim(otw, ctx);
    let mut d = display::new_with_fields<Foo>(
      &publisher,
      vector[
        utf8(b"bar"),       // Too big to display
        utf8(b"bar_val"),   // ... but it's still fine to extract information from it
        utf8(b"baz"),       // Other fields after the big one are fine too
        utf8(b"qux"),       // Vectors aren't supported.
        utf8(b"quy"),       // Doesn't exist
      ],
      vector[
        utf8(b"{bar}"),
        utf8(b"{bar.val}"),
        utf8(b"{baz}"),
        utf8(b"{qux}"),
        utf8(b"{quy}"),
      ],
      ctx,
    );

    d.update_version();
    transfer::public_transfer(publisher, ctx.sender());
    transfer::public_transfer(d, ctx.sender());
  }

  public fun new(bar: u8, baz: u64, qux: u64, ctx: &mut TxContext): Foo {
    let bar = Bar {
      chunky: vector::tabulate!(4 * 1024, |_| chunky(chunky(chunky(chunky(bar))))),
      val: bar as u64,
    };

    let qux = vector::tabulate!(qux, |i| i);

    Foo { id: object::new(ctx), bar, baz, qux }
  }

  fun chunky<T: store>(x: T): Chunky<T> {
    Chunky {
      long_field_name_0000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000: x
    }
  }
}

//# programmable --sender A --inputs 42u8 43 44 @A
//> 0: test::mod::new(Input(0), Input(1), Input(2));
//> 1: TransferObjects([Result(0)], Input(3))

//# create-checkpoint

//# run-graphql
{
  object(address: "@{obj_2_0}") {
    asMoveObject {
      contents {
        display {
          output
          errors
        }
      }
    }
  }
}
