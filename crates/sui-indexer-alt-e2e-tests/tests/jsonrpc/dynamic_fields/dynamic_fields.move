// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

//# init --protocol-version 70 --accounts A --addresses P=0x0 --simulator

// 1. Fetch all dynamic fields
// 2. Fetch a limited page of dynamic fields.
// 3. Fetch a limited page of dynamic fields with a cursor.
// 4. Fetch all remaining dynamic fields after a cursor.
// 5. Add a dynamic field, and fetch all dynamic fields.
// 6. Remove a dynamic field and fetch all dynamic fields.
// 7. Modify a dynamic field and fetch all dynamic fields.

//# publish
module P::M {
  use sui::dynamic_field as df;
  use sui::dynamic_object_field as dof;

  public struct Object has key, store {
    id: UID,
  }

  public struct Name has copy, drop, store {
    x: u64,
    y: u32,
  }

  public fun object(ctx: &mut TxContext): Object {
    Object { id: object::new(ctx) }
  }

  public fun dfield<K: copy + drop + store, V: store>(o: &mut Object, k: K, v: V) {
    df::add<K, V>(&mut o.id, k, v);
  }

  public fun ofield<K: copy + drop + store, V: key + store>(o: &mut Object, k: K, v: V) {
    dof::add<K, V>(&mut o.id, k, v);
  }

  public fun dmodify<K: copy + drop + store>(o: &mut Object, k: K) {
    let v: &mut u64 = df::borrow_mut<K, u64>(&mut o.id, k);
    *v = *v + 1;
  }

  public fun dremove<K: copy + drop + store, V: drop + store>(o: &mut Object, k: K) {
    df::remove<K, V>(&mut o.id, k);
  }

  public fun name(x: u64, y: u32): Name {
    Name { x, y }
  }
}

//# programmable --sender A --inputs @A
//> 0: P::M::object();
//> 1: TransferObjects([Result(0)], Input(0))

//# programmable --sender A --inputs object(2,0) 42 43u32 44
//> 0: P::M::name(Input(1), Input(2));
//> 1: P::M::dfield<P::M::Name, u64>(Input(0), Result(0), Input(3))

//# create-checkpoint

//# programmable --sender A --inputs @A
//> 0: P::M::object();
//> 1: TransferObjects([Result(0)], Input(0))

//# programmable --sender A --inputs object(2,0) 45 object(5,0)
//> 0: P::M::ofield<u64, P::M::Object>(Input(0), Input(1), Input(2))

//# create-checkpoint

//# programmable --sender A --inputs object(2,0) 46 47
//> 0: P::M::dfield<u64, u64>(Input(0), Input(1), Input(2))

//# create-checkpoint

//# programmable --sender A --inputs object(2,0) true 48
//> 0: P::M::dfield<bool, u64>(Input(0), Input(1), Input(2))

//# create-checkpoint

//# programmable --sender A --inputs object(2,0) false 49
//> 0: P::M::dfield<bool, u64>(Input(0), Input(1), Input(2))

//# create-checkpoint

//# run-jsonrpc
{
  "method": "sui_multiGetObjects",
  "params": [
    [
      "@{obj_3_0}",
      "@{obj_5_0}",
      "@{obj_6_0}",
      "@{obj_8_0}",
      "@{obj_10_0}",
      "@{obj_12_0}"
    ],
    {
      "showOwner": true,
      "showType": true,
      "showContent": true
    }
  ]
}

//# run-jsonrpc
{
  "method": "suix_getDynamicFields",
  "params": ["@{obj_2_0}"]
}

//# run-jsonrpc
{
  "method": "suix_getDynamicFields",
  "params": ["@{obj_2_0}", null, 2]
}

//# run-jsonrpc --cursors bcs(@{obj_10_0},4)
{
  "method": "suix_getDynamicFields",
  "params": ["@{obj_2_0}", "@{cursor_0}", 2]
}

//# run-jsonrpc --cursors bcs(@{obj_10_0},4)
{
  "method": "suix_getDynamicFields",
  "params": ["@{obj_2_0}", "@{cursor_0}"]
}

//# programmable --sender A --inputs object(2,0) 51 52
//> 0: P::M::dfield<u64, u64>(Input(0), Input(1), Input(2))

//# create-checkpoint

//# run-jsonrpc
{
  "method": "suix_getDynamicFields",
  "params": ["@{obj_2_0}"]
}

//# programmable --sender A --inputs object(2,0) true
//> 0: P::M::dremove<bool, u64>(Input(0), Input(1))

//# create-checkpoint

//# run-jsonrpc
{
  "method": "suix_getDynamicFields",
  "params": ["@{obj_2_0}"]
}

//# programmable --sender A --inputs object(2,0) false
//> 0: P::M::dmodify<bool>(Input(0), Input(1))

//# create-checkpoint

//# run-jsonrpc
{
  "method": "suix_getDynamicFields",
  "params": ["@{obj_2_0}"]
}

//# run-jsonrpc
{
  "method": "sui_multiGetObjects",
  "params": [
    [
      "@{obj_12_0}",
      "@{obj_19_0}"
    ],
    {
      "showOwner": true,
      "showType": true,
      "showContent": true
    }
  ]
}
