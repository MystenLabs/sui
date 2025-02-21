// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

//# init --protocol-version 70 --accounts A --addresses P0=0x0 --simulator

// "created and wrapped" as well as "unwrapped and deleted" objects do not show
// up in object changes at all.

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
    O { id: object::new(ctx), i: option::none() }
  }

  public fun wrap(o: &mut O, ctx: &mut TxContext) {
    o.i.fill(I { id: object::new(ctx) });
  }

  public fun unwrap(o: &mut O): I {
    o.i.extract()
  }

  public fun delete(i: I) {
    let I { id } = i;
    id.delete();
  }
}

//# programmable --sender A --inputs @A
//> 0: P0::M::new();
//> 1: TransferObjects([Result(0)], Input(0))

//# programmable --sender A --inputs object(2,0)
//> 0: P0::M::wrap(Input(0))

//# programmable --sender A --inputs object(2,0)
//> 0: P0::M::unwrap(Input(0));
//> 1: P0::M::delete(Result(0))

//# create-checkpoint

//# run-jsonrpc
{
  "method": "sui_getTransactionBlock",
  "params": ["@{digest_3}", { "showObjectChanges": true }]
}

//# run-jsonrpc
{
  "method": "sui_getTransactionBlock",
  "params": ["@{digest_4}", { "showObjectChanges": true }]
}
