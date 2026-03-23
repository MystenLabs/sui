// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

//# init --protocol-version 108 --accounts A --addresses test=0x0 --simulator

//# publish --sender A
module test::mod {
  use sui::derived_object;

  public struct Parent has key, store {
    id: UID,
  }

  public struct Child has key, store {
    id: UID,
    x: u64,
    y: u64,
  }

  public fun new(ctx: &mut TxContext): Parent {
    Parent { id: object::new(ctx) }
  }

  public fun add_child(parent: &mut Parent, key: u64, x: u64, y: u64, ctx: &mut TxContext) {
    let child = Child {
      id: derived_object::claim(&mut parent.id, key),
      x,
      y,
    };
    transfer::public_transfer(child, ctx.sender());
  }
}

//# programmable --sender A --inputs @A
//> 0: test::mod::new();
//> 1: TransferObjects([Result(0)], Input(0))

//# programmable --sender A --inputs object(2,0) 7u64 111u64 222u64
//> 0: test::mod::add_child(Input(0), Input(1), Input(2), Input(3))

//# create-checkpoint

//# run-graphql
{
  object(address: "@{obj_2_0}") {
    asMoveObject {
      contents {
        formatted: format(format: "{id~>[7u64].x}")
        missingFormatted: format(format: "{id~>[8u64].x}")
        extracted: extract(path: "id~>[7u64]") {
          json
        }
        missingExtracted: extract(path: "id~>[8u64]") {
          json
        }
      }
    }
  }
}
