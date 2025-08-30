// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

//# init --protocol-version 70 --accounts A --addresses test=0x0 --simulator

// Display imposes a limit on how deeply nested field accesses can be (default
// to 10). If a Display format contains a nesting greater than this, showing
// Display will fail.

// 1. Publish a package that includes a Display format with a field that is too
//    deeply nested.
// 2. Create an object from this package.
// 3. Try to view the contents of the new object -- this will succeed.
// 4. Try to view its display, which will fail because the field access is
//    deeper than the limit.

//# publish --sender A
module test::mod {
  use std::string::utf8;
  use sui::display;
  use sui::package;

  public struct MOD() has drop;

  public struct Foo has key, store {
    id: UID,
    deep: Deep<Deep<Deep<Deep<Deep<Deep<Deep<Deep<Deep<Deep<Deep<u8>>>>>>>>>>>,
  }


  public struct Deep<T: store> has store {
    deep: T,
  }

  fun init(otw: MOD, ctx: &mut TxContext) {
    let publisher = package::claim(otw, ctx);
    let mut d = display::new_with_fields<Foo>(
      &publisher,
      // Too deep to display
      vector[utf8(b"deep")],
      vector[utf8(b"{deep.deep.deep.deep.deep.deep.deep.deep.deep.deep.deep}")],
      ctx,
    );

    d.update_version();
    transfer::public_transfer(publisher, ctx.sender());
    transfer::public_transfer(d, ctx.sender());
  }

  public fun new(d: u8, ctx: &mut TxContext): Foo {
    Foo {
      id: object::new(ctx),
      deep: deep(deep(deep(deep(deep(deep(deep(deep(deep(deep(deep(d)))))))))))
    }
  }

  fun deep<T: store>(deep: T): Deep<T> {
    Deep { deep }
  }
}

//# programmable --sender A --inputs 42u8 @A
//> 0: test::mod::new(Input(0));
//> 1: TransferObjects([Result(0)], Input(1))

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
