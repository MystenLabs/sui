// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

//# init --accounts A --addresses test=0x0 --simulator --protocol-version 118

//# publish --sender A
module test::mod {
  use std::type_name::{Self, TypeName};

  public struct Foo has key, store {
    id: UID,
    tn: TypeName,
  }

  public fun new(ctx: &mut TxContext): Foo {
    Foo {
      id: object::new(ctx),
      tn: type_name::with_original_ids<vector<Foo>>(),
    }
  }
}

//# programmable --sender A --inputs @A
//> 0: test::mod::new();
//> 1: TransferObjects([Result(0)], Input(0))

//# create-checkpoint

//# run-graphql
{
  object(address: "@{obj_2_0}") {
    asMoveObject {
      contents {
        json
        typeAsJson: format(format: "{tn:json}")
        typeAsStr: format(format: "{tn}")
      }
    }
  }
}
