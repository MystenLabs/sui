// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

//# init --protocol-version 70 --accounts A --addresses P=0x0 --simulator

//# publish
module P::M {
  public struct Foo has key {
    id: UID,
  }

  public fun new(ctx: &mut TxContext) {
    transfer::transfer(Foo { id: object::new(ctx) }, ctx.sender());
  }
}

//# programmable --sender A --inputs @A
//> P::M::new();

//# create-checkpoint

//# run-graphql
{
  coinHasPublicTransfer: object(address: "@{obj_0_0}") {
    asMoveObject {
      contents { type { repr abilities } }
      hasPublicTransfer
    }
  }

  fooDoesNot: object(address: "@{obj_2_0}") {
    asMoveObject {
      contents { type { repr abilities } }
      hasPublicTransfer
    }
  }
}
