// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

//# init --addresses Test=0x0 --accounts A --simulator --custom-validator-account --reference-gas-price 234 --default-gas-price 1000

//# publish
module Test::M1 {
    use sui::dynamic_object_field as ofield;
    use sui::object::{Self, UID};
    use sui::tx_context::TxContext;
    use sui::transfer;

    struct Parent has key, store {
        id: UID,
    }

    struct Child has key, store {
        id: UID,
        count: u64,
    }

    public entry fun parent(recipient: address, ctx: &mut TxContext) {
        transfer::public_transfer(
            Parent { id: object::new(ctx) },
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

    public fun mutate_child(child: &mut Child) {
        child.count = child.count + 1;
    }

    public fun mutate_child_via_parent(parent: &mut Parent, name: u64) {
        mutate_child(ofield::borrow_mut(&mut parent.id, name))
    }

    public fun reclaim_child(parent: &mut Parent, name: u64): Child {
        ofield::remove(&mut parent.id, name)
    }

    public fun delete_child(parent: &mut Parent, name: u64) {
        let Child { id, count: _ } = reclaim_child(parent, name);
        object::delete(id);
    }
}

//# programmable --sender A --inputs @A 42
//> 0: Test::M1::parent(Input(0));
//> 1: Test::M1::child(Input(0));

//# run Test::M1::add_child --sender A --args object(2,1) object(2,0) 1

//# create-checkpoint

//# run Test::M1::mutate_child_via_parent --sender A --args object(2,1) 1

//# create-checkpoint

//# run-graphql
# Child should have value of 1
{
  object(address: "@{obj_2_1}") {
    dynamicFields {
      nodes {
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
    }
  }
}

//# run Test::M1::mutate_child_via_parent --sender A --args object(2,1) 1

//# create-checkpoint

//# run-graphql
# Child should have value of 2
{
  object(address: "@{obj_2_1}") {
    dynamicFields {
      nodes {
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
    }
  }
}

//# run-graphql
# View the parent at the previous version, which should be a child with a value of 1
{
  object(address: "@{obj_2_1}", version: 4) {
    dynamicFields {
      nodes {
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
    }
  }
}
