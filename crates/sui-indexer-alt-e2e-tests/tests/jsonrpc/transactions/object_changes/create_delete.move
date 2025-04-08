// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

//# init --protocol-version 70 --accounts A --addresses P0=0x0 --simulator

// Test creating and deleting an object. Both of these operations should show
// up in object changes.

//# publish
module P0::M {
  public struct O has key, store {
    id: UID,
  }

  public fun new(ctx: &mut TxContext): O {
    O { id: object::new(ctx) }
  }

  public fun delete(o: O) {
    let O { id } = o;
    id.delete();
  }
}

//# programmable --sender A --inputs @A
//> 0: P0::M::new();
//> 1: TransferObjects([Result(0)], Input(0))

//# programmable --sender A --inputs object(2,0)
//> 0: P0::M::delete(Input(0))

//# create-checkpoint

//# run-jsonrpc
{
  "method": "sui_getTransactionBlock",
  "params": ["@{digest_2}", { "showObjectChanges": true }]
}

//# run-jsonrpc
{
  "method": "sui_getTransactionBlock",
  "params": ["@{digest_3}", { "showObjectChanges": true }]
}
