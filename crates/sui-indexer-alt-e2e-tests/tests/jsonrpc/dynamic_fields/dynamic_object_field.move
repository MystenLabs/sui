// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

//# init --protocol-version 70 --accounts A --addresses P=0x0 --simulator

// 1. Look for a dynamic field with a simple key
// 2. Look for a dynamic field with a struct-based key

//# publish
module P::M {
  public struct Name has copy, drop, store {
    x: u64,
    y: u32,
  }

  public struct Value has key, store {
    id: UID,
    x: u64
  }

  public fun name(x: u64, y: u32): Name {
    Name { x, y }
  }

  public fun value(x: u64, ctx: &mut TxContext): Value {
    Value { id: object::new(ctx), x }
  }
}

//# programmable --sender A --inputs @A
//> 0: sui::object_bag::new();
//> 1: TransferObjects([Result(0)], Input(0))

//# programmable --sender A --inputs object(2,0) 42 43u32 44 45 46 @A
//> 0: P::M::name(Input(1), Input(2));
//> 1: P::M::value(Input(4));
//> 2: P::M::value(Input(5));
//> 3: sui::object_bag::add<u64, P::M::Value>(Input(0), Input(3), Result(1));
//> 4: sui::object_bag::add<P::M::Name, P::M::Value>(Input(0), Result(0), Result(2))

//# create-checkpoint

//# run-jsonrpc
{
  "method": "suix_getDynamicFieldObject",
  "params": ["@{obj_2_0}", { "type": "u64", "value": "44" }]
}

//# run-jsonrpc
{
  "method": "suix_getDynamicFieldObject",
  "params": [
    "@{obj_2_0}",
    { "type": "@{P}::M::Name", "value": { "x": "42", "y": 43 } }
  ]
}
