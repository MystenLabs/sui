// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

//# init --protocol-version 70 --accounts A B --addresses P=0x0 --simulator

// 1. All the objects of a generic type instance, for a particular owner
// 2. All the objects of another generic type instance, for that same owner
// 3. ...limited
// 4. ...limited, with a cursor

//# publish
module P::M {
  public struct O1<phantom T> has key, store {
    id: UID,
  }

  public fun o1<T>(ctx: &mut TxContext): O1<T> {
    O1 { id: object::new(ctx) }
  }
}

//# programmable --sender B --inputs @B
//> 0: P::M::o1<u64>();
//> 1: TransferObjects([Result(0)], Input(0))

//# programmable --sender A --inputs @A
//> 0: P::M::o1<u64>();
//> 1: P::M::o1<u32>();
//> 2: P::M::o1<u64>();
//> 3: TransferObjects([Result(0), Result(1), Result(2)], Input(0))

//# create-checkpoint

//# programmable --sender A --inputs @A
//> 0: P::M::o1<u32>();
//> 1: TransferObjects([Result(0)], Input(0))

//# create-checkpoint

//# run-jsonrpc
{
  "method": "suix_getOwnedObjects",
  "params": [
    "@{A}",
    {
      "filter": { "StructType": "@{P}::M::O1<u64>" },
      "options": { "showType": true }
    }
  ]
}

//# run-jsonrpc
{
  "method": "suix_getOwnedObjects",
  "params": [
    "@{A}",
    {
      "filter": { "StructType": "@{P}::M::O1<u32>" },
      "options": { "showType": true }
    }
  ]
}

//# run-jsonrpc
{
  "method": "suix_getOwnedObjects",
  "params": [
    "@{A}",
    {
      "filter": { "StructType": "@{P}::M::O1<u32>" },
      "options": { "showType": true }
    },
    null,
    1
  ]
}

//# run-jsonrpc --cursors bcs(@{obj_5_0},2)
{
  "method": "suix_getOwnedObjects",
  "params": [
    "@{A}",
    {
      "filter": { "StructType": "@{P}::M::O1<u32>" },
      "options": { "showType": true }
    },
    "@{cursor_0}",
    1
  ]
}
