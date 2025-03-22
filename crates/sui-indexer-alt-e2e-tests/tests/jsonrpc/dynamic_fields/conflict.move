// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

//# init --protocol-version 70 --accounts A --addresses P=0x0 --simulator

// Set-up an object that has a dynamic field and a dynamic object field with
// the same key. JSON-RPC cannot distinguish between the two and will always
// preference the dynamic field.

// 1. Look for a dynamic field with a simple key
// 2. Look for a dynamic field with a struct-based key

//# publish
module P::M {
  use sui::dynamic_field as df;
  use sui::dynamic_object_field as dof;

  public struct Owner has key, store {
    id: UID
  }

  public struct Value has key, store {
    id: UID,
    value: u64
  }

  public fun owner(ctx: &mut TxContext): Owner {
    Owner { id: object::new(ctx) }
  }

  public fun add_df(owner: &mut Owner, key: u64, value: u64, ctx: &mut TxContext) {
    df::add(&mut owner.id, key, Value { id: object::new(ctx), value });
  }

  public fun add_dof(owner: &mut Owner, key: u64, value: u64, ctx: &mut TxContext) {
    dof::add(&mut owner.id, key, Value { id: object::new(ctx), value });
  }
}

//# programmable --sender A --inputs @A
//> 0: P::M::owner();
//> 1: TransferObjects([Result(0)], Input(0))

//# programmable --sender A --inputs object(2,0) 42 43 44
//> 0: P::M::add_df(Input(0), Input(1), Input(2));
//> 1: P::M::add_dof(Input(0), Input(1), Input(3))

//# create-checkpoint

//# run-jsonrpc
{
  "method": "suix_getDynamicFieldObject",
  "params": ["@{obj_2_0}", { "type": "u64", "value": "42" }]
}
