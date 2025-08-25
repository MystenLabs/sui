// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

//# init --protocol-version 70 --accounts A --addresses test=0x0 --simulator

// Trying to showDisplay when,
// 1. the type has no Display format,
// 2. the type is primitive (and could never have a Display format).

//# publish
module test::mod {
  public struct Foo has key, store {
    id: UID,
  }

  public fun new(ctx: &mut TxContext): Foo {
    Foo { id: object::new(ctx) }
  }
}

//# programmable --sender A --inputs @A
//> 0: test::mod::new();
//> 1: TransferObjects([Result(0)], Input(0))

//# programmable --sender A --inputs @A
//> 0: sui::bag::new();
//> 1: TransferObjects([Result(0)], Input(0))

//# programmable --sender A --inputs object(3,0) 42
//> 0: sui::bag::add<u64, u64>(Input(0), Input(1), Input(1));

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

//# run-graphql --cursors bcs(42u64)
{
  address(address: "@{obj_3_0}") {
    dynamicField(name: { type: "u64", bcs: "@{cursor_0}" }) {
      name {
        json
        display {
          output
          errors
        }
      }
    }
  }
}
