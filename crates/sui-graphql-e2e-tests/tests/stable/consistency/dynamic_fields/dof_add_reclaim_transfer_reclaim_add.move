// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

// Test that we do not return the child object when it is not owned by the parent or when it is
// wrapped.

// parent version | child version | status
// ---------------|---------------|--------
// 2              | 2             | created parent and child
// 3              | 3             | added child to parent
// 4              | 4             | reclaimed child from parent
// 4              | 5             | add child to another parent
// 4              | 6             | reclaim child from another parent
// 7              | 7             | add child to original parent

//# init --protocol-version 51 --addresses Test=0x0 --accounts A --simulator

//# publish
module Test::M1 {
    use sui::dynamic_object_field as ofield;

    public struct Parent has key, store {
        id: UID,
        count: u64
    }

    public struct Child has key, store {
        id: UID,
        count: u64,
    }

    public entry fun parent(recipient: address, ctx: &mut TxContext) {
        transfer::public_transfer(
            Parent { id: object::new(ctx), count: 0 },
            recipient
        )
    }

    public entry fun child(recipient: address, ctx: &mut TxContext) {
        transfer::public_transfer(
            Child { id: object::new(ctx), count: 0 },
            recipient
        )
    }

    public fun add_child(parent: &mut Parent, child: Child, name: u64) {
        ofield::add(&mut parent.id, name, child);
    }

    public fun reclaim_child(parent: &mut Parent, name: u64): Child {
        ofield::remove(&mut parent.id, name)
    }

    public fun reclaim_and_transfer_child(parent: &mut Parent, name: u64, recipient: address) {
        transfer::public_transfer(reclaim_child(parent, name), recipient)
    }
}

//# programmable --sender A --inputs @A
//> 0: Test::M1::child(Input(0));
//> 1: Test::M1::parent(Input(0));
//> 2: Test::M1::parent(Input(0));

//# run Test::M1::add_child --sender A --args object(2,1) object(2,0) 42

//# run Test::M1::reclaim_and_transfer_child --sender A --args object(2,1) 42 @A

//# run Test::M1::add_child --sender A --args object(2,2) object(2,0) 42

//# run Test::M1::reclaim_and_transfer_child --sender A --args object(2,2) 42 @A

//# run Test::M1::add_child --sender A --args object(2,1) object(2,0) 42

//# create-checkpoint

//# run-graphql
fragment DynamicFieldSelect on DynamicField {
  name {
    bcs
  }
  value {
    ... on MoveObject {
      contents {
        json
      }
    }
    ... on MoveValue {
      json
    }
  }
}

fragment DynamicFieldsSelect on DynamicFieldConnection {
  edges {
    cursor
    node {
      ...DynamicFieldSelect
    }
  }
}

{
  latest: object(address: "@{obj_2_1}") {
    version
    dynamicFields {
      ...DynamicFieldsSelect
    }
    dynamicObjectField(name: {type: "u64", bcs: "KgAAAAAAAAA="}) {
        ...DynamicFieldSelect
    }
  }
  owner_view: owner(address: "@{obj_2_1}") {
    dynamicFields {
      ...DynamicFieldsSelect
    }
    dynamicObjectField(name: {type: "u64", bcs: "KgAAAAAAAAA="}) {
        ...DynamicFieldSelect
    }
  }
  v2: object(address: "@{obj_2_1}", version: 2) {
    version
    dynamicFields {
      ...DynamicFieldsSelect
    }
    dynamicObjectField(name: {type: "u64", bcs: "KgAAAAAAAAA="}) {
        ...DynamicFieldSelect
    }
  }
  v3: object(address: "@{obj_2_1}", version: 3) {
    version
    dynamicFields {
      ...DynamicFieldsSelect
    }
    dynamicObjectField(name: {type: "u64", bcs: "KgAAAAAAAAA="}) {
        ...DynamicFieldSelect
    }
  }
  v4: object(address: "@{obj_2_1}", version: 4) {
    version
    dynamicFields {
      ...DynamicFieldsSelect
    }
    dynamicObjectField(name: {type: "u64", bcs: "KgAAAAAAAAA="}) {
        ...DynamicFieldSelect
    }
  }
  v7: object(address: "@{obj_2_1}", version: 7) {
    version
    dynamicFields {
      ...DynamicFieldsSelect
    }
    dynamicObjectField(name: {type: "u64", bcs: "KgAAAAAAAAA="}) {
        ...DynamicFieldSelect
    }
  }
}
