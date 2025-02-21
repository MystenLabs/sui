// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

//# init --protocol-version 70 --accounts A --addresses P0=0x0 --simulator

// Unwrapped objects don't show up in object changes.

//# publish
module P0::M {
  public struct O has key, store {
    id: UID,
    i: Option<I>,
  }

  public struct I has key, store {
    id: UID,
  }

  public fun new(ctx: &mut TxContext): O {
    let i = I { id: object::new(ctx) };
    O { id: object::new(ctx), i: option::some(i) }
  }

  public fun unwrap(o: &mut O): I {
    o.i.extract()
  }
}

//# programmable --sender A --inputs @A
//> 0: P0::M::new();
//> 1: TransferObjects([Result(0)], Input(0))

//# programmable --sender A --inputs object(2,0) @A
//> 0: P0::M::unwrap(Input(0));
//> 1: TransferObjects([Result(0)], Input(1))

//# create-checkpoint

//# run-jsonrpc
{
  "method": "sui_getTransactionBlock",
  "params": ["@{digest_3}", { "showObjectChanges": true }]
}
