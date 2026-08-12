// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

//# init --protocol-version 108 --accounts A --addresses test=0x0 --simulator

//# publish --sender A
module test::derived {
  use sui::derived_object;

  public struct Parent has key, store {
    id: UID,
  }

  public struct Child has key, store {
    id: UID,
    key: u64,
    value: u64,
  }

  public fun new(ctx: &mut TxContext): Parent {
    Parent { id: object::new(ctx) }
  }

  public fun claim(parent: &mut Parent, key: u64): Child {
    Child {
      id: derived_object::claim(&mut parent.id, key),
      key,
      value: key,
    }
  }

  public fun set_value(child: &mut Child, value: u64) {
    child.value = value;
  }
}

//# programmable --sender A --inputs @A
//> 0: test::derived::new();
//> 1: TransferObjects([Result(0)], Input(0));

//# programmable --sender A --inputs object(2,0) 7u64 8u64 9u64 @A
//> 0: test::derived::claim(Input(0), Input(1));
//> 1: test::derived::claim(Input(0), Input(2));
//> 2: test::derived::claim(Input(0), Input(3));
//> 3: TransferObjects([Result(0), Result(1), Result(2)], Input(4));

//# create-checkpoint

//# programmable --sender A --inputs object(3,3) object(3,4) object(3,5) 70u64
//> 0: test::derived::set_value(Input(0), Input(3));
//> 1: test::derived::set_value(Input(1), Input(3));
//> 2: test::derived::set_value(Input(2), Input(3));

//# create-checkpoint

//# run-graphql
{
  address(address: "@{obj_2_0}") {
    ...DerivedObjectLookups
    sevenAtCheckpoint: derivedObject(name: { literal: "7u64" }, atCheckpoint: 1) { ...Child }
    sevenAtVersion: derivedObject(name: { literal: "7u64" }, version: 4) { ...Child }
    sevenAtRootVersion: derivedObject(name: { literal: "7u64" }, rootVersion: 4) { ...Child }
  }
}

fragment DerivedObjectLookups on IAddressable {
  seven: derivedObject(name: { literal: "7u64" }) { ...Child }
  eight: derivedObject(name: { literal: "8u64" }) { ...Child }
  nine: derivedObject(name: { literal: "9u64" }) { ...Child }
  missing: derivedObject(name: { literal: "10u64" }) { ...Child }
  multiGetDerivedObjects(keys: [
    { name: { literal: "7u64" } },
    { name: { literal: "8u64" } },
    { name: { literal: "9u64" } },
    { name: { literal: "10u64" } },
    { name: { literal: "7u64" }, atCheckpoint: 1 },
    { name: { literal: "7u64" }, version: 4 },
    { name: { literal: "7u64" }, rootVersion: 4 },
  ]) { ...Child }
}

fragment Child on MoveObject {
  address
  contents { json }
}

//# run-graphql
{
  object(address: "@{obj_2_0}") {
    latest: derivedObject(name: { literal: "7u64" }) { ...Child }
    atCheckpoint: derivedObject(name: { literal: "7u64" }, atCheckpoint: 1) { ...Child }
    atVersion: derivedObject(name: { literal: "7u64" }, version: 4) { ...Child }
    atRootVersion: derivedObject(name: { literal: "7u64" }, rootVersion: 4) { ...Child }
    multiGetDerivedObjects(keys: [
      { name: { literal: "7u64" } },
      { name: { literal: "7u64" }, atCheckpoint: 1 },
      { name: { literal: "7u64" }, version: 4 },
      { name: { literal: "7u64" }, rootVersion: 4 },
    ]) { ...Child }
  }
}

fragment Child on MoveObject {
  address
  contents { json }
}

//# run-graphql --cursors bcs(8u64)
{ # Fetch derived objects directly from their parent/name keys
  multiGetDerivedObjects(keys: [
    { parent: "@{obj_2_0}", name: { literal: "7u64" } },
    { parent: "@{obj_2_0}", name: { type: "u64", bcs: "@{cursor_0}" } },
    { parent: "@{obj_2_0}", name: { literal: "9u64" } },
    { parent: "@{obj_2_0}", name: { literal: "10u64" } },
    { parent: "@{obj_2_0}", name: { literal: "7u64" } },
    { parent: "@{obj_2_0}", name: { literal: "7u64" } },
    { parent: "@{obj_2_0}", name: { literal: "7u64" }, atCheckpoint: 1 },
    { parent: "@{obj_2_0}", name: { literal: "7u64" }, version: 4 },
    { parent: "@{obj_2_0}", name: { literal: "7u64" }, rootVersion: 4 },
  ]) { ...Child }
}

fragment Child on MoveObject {
  address
  contents { json }
}
