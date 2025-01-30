// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

//# init --protocol-version 70 --accounts A B --addresses P0=0x0 --simulator

// When an object's owner changes, it is considered "transferred".

//# publish
module P0::M {
  public struct O has key, store {
    id: UID,
  }

  public fun new(ctx: &mut TxContext): O {
    O { id: object::new(ctx) }
  }
}

//# programmable --sender A --inputs @A
//> 0: P0::M::new();
//> 1: TransferObjects([Result(0)], Input(0))

//# programmable --sender A --inputs object(2,0) @B
//> 0: TransferObjects([Input(0)], Input(1))

//# create-checkpoint

//# run-jsonrpc
{
  "method": "sui_getTransactionBlock",
  "params": ["@{digest_3}", { "showObjectChanges": true }]
}
