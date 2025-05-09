// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

//# init --protocol-version 70 --accounts A --addresses P=0x0 --simulator

//# publish
module P::M {
  public struct T has key, store {
    id: UID,
    x: u64
  }

  public fun new(x: u64, ctx: &mut TxContext): T {
    T { id: object::new(ctx), x }
  }
}

//# programmable --sender A --inputs 42 @A
//> 0: P::M::new(Input(0));
//> 1: TransferObjects([Result(0)], Input(1))

//# programmable --sender A --inputs 43 @A
//> 0: P::M::new(Input(0));
//> 1: TransferObjects([Result(0)], Input(1))

//# view-object 2,0

//# view-object 3,0

//# create-checkpoint

//# run-graphql
{
  a: object(address: "@{obj_2_0}", version: 2) {
    objectBcs
  }

  b: object(address: "@{obj_3_0}", version: 3) {
    objectBcs
  }

  multiGetObjects(keys: [
    { address: "@{obj_2_0}", version: 2 },
    { address: "@{obj_3_0}", version: 3 }
  ]) {
    objectBcs
  }
}
