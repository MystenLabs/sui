// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

//# init --protocol-version 70 --accounts A B --addresses P=0x0 --simulator

// 1. All the objects of a certain module, for a particular owner
// 2. All the objects of another module, for that same owner
// 3. ...limited
// 4. ...limited, with a cursor

//# publish
module P::M {
  public struct O1 has key, store {
    id: UID,
  }

  public struct O2 has key, store {
    id: UID,
  }

  public fun o1(ctx: &mut TxContext): O1 {
    O1 { id: object::new(ctx) }
  }

  public fun o2(ctx: &mut TxContext): O2 {
    O2 { id: object::new(ctx) }
  }
}

module P::N {
  public struct O1 has key, store {
    id: UID,
  }

  public fun o1(ctx: &mut TxContext): O1 {
    O1 { id: object::new(ctx) }
  }
}

//# programmable --sender B --inputs @B
//> 0: P::M::o1();
//> 1: TransferObjects([Result(0)], Input(0))

//# programmable --sender A --inputs @A
//> 0: P::M::o1();
//> 1: P::M::o2();
//> 2: P::N::o1();
//> 3: TransferObjects([Result(0), Result(1), Result(2)], Input(0))

//# create-checkpoint

//# programmable --sender A --inputs @A
//> 0: P::M::o1();
//> 1: TransferObjects([Result(0)], Input(0))

//# create-checkpoint

//# run-jsonrpc
{
  "method": "suix_getOwnedObjects",
  "params": [
    "@{A}",
    {
      "filter": { "MoveModule": ["@{P}", "M"] },
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
      "filter": { "MoveModule": ["@{P}", "N"] },
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
      "filter": { "MoveModule": ["@{P}", "M"] },
      "options": { "showType": true }
    },
    null,
    1
  ]
}

//# run-jsonrpc --cursors @{obj_5_0,2}
{
  "method": "suix_getOwnedObjects",
  "params": [
    "@{A}",
    {
      "filter": { "MoveModule": ["@{P}", "M"] },
      "options": { "showType": true }
    },
    "@{cursor_0}",
    1
  ]
}
