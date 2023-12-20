// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

//# init --addresses Test=0x0 --accounts A --simulator

//# publish
module Test::m {
  use std::string;
    use sui::dynamic_field as field;
    use sui::dynamic_object_field as ofield;
    use sui::object;
    use sui::tx_context::{sender, TxContext};

    struct Wrapper has key {
        id: object::UID,
        o: Parent
    }

    struct Parent has key, store {
        id: object::UID,
    }

    struct Child has key, store {
        id: object::UID,
    }

    struct Cup<T> has key, store {
        id: object::UID,
        value: T
    }

    struct Name<T> has copy, drop, store {
        value: T
    }

    public entry fun create_obj(ctx: &mut TxContext){
        let id = object::new(ctx);
        sui::transfer::public_transfer(Parent { id }, sender(ctx))
    }

    public entry fun add_df(obj: &mut Parent) {
        let id = &mut obj.id;
        field::add<u64, u64>(id, 0, 0);
        field::add<vector<u8>, u64>(id, b"", 1);
        field::add<bool, u64>(id, false, 2);
    }

    public entry fun add_dof(parent: &mut Parent, ctx: &mut TxContext) {
        let child = Child { id: object::new(ctx) };
        ofield::add(&mut parent.id, 0, child);
    }

    public entry fun add_cup_num(parent: &mut Parent, value: u64, ctx: &mut TxContext) {
        let cup = Cup { id: object::new(ctx), value };
        let name = Name { value };
        ofield::add(&mut parent.id, name, cup);
    }

    public entry fun add_cup_string(parent: &mut Parent, value: string::String, ctx: &mut TxContext) {
        let cup = Cup { id: object::new(ctx), value };
        let name = Name { value };
        ofield::add(&mut parent.id, name, cup);
    }
}

//# run Test::m::create_obj --sender A

//# run Test::m::add_df --sender A --args object(2,0)

//# run Test::m::add_dof --sender A --args object(2,0)

//# run Test::m::add_cup_num --sender A --args object(2,0) 42

//# run Test::m::add_cup_string --sender A --args object(2,0) b"Will"

//# create-checkpoint

//# view-checkpoint

//# run-graphql
{
  # should yield the 1 df of Field<u64, u64>
  object(address: "@{obj_2_0}") {
    dynamicFieldConnection(filter: { nameType: "u64", valueType: "u64" }) {
      nodes {
        name {
          type {
            repr
          }
          data
          bcs
        }
        value {
          ... on MoveObject {
            __typename
          }
          ... on MoveValue {
            __typename
          }
        }
      }
    }
  }
}

//# run-graphql
{
  # should yield the single Child dof
  object(address: "@{obj_2_0}") {
    dynamicFieldConnection(filter: { nameType: "u64", valueType: "@{Test}" }) {
      nodes {
        name {
          type {
            repr
          }
          data
          bcs
        }
        value {
          ... on MoveObject {
            __typename
          }
          ... on MoveValue {
            __typename
          }
        }
      }
    }
  }
}


//# run-graphql
{
  # should yield the 2 Cup dofs
  object(address: "@{obj_2_0}") {
    dynamicFieldConnection(filter: { nameType: "@{Test}", valueType: "@{Test}" }) {
      nodes {
        name {
          type {
            repr
          }
          data
          bcs
        }
        value {
          ... on MoveObject {
            __typename
          }
          ... on MoveValue {
            __typename
          }
        }
      }
    }
  }
}

//# run-graphql
{
  # should yield the 2 Cup dofs
  object(address: "@{obj_2_0}") {
    dynamicFieldConnection(filter: { nameType: "@{Test}::m", valueType: "@{Test}::m" }) {
      nodes {
        name {
          type {
            repr
          }
          data
          bcs
        }
        value {
          ... on MoveObject {
            __typename
          }
          ... on MoveValue {
            __typename
          }
        }
      }
    }
  }
}

//# run-graphql
{
  # should yield the 2 Cup dofs
  object(address: "@{obj_2_0}") {
    dynamicFieldConnection(filter: { nameType: "@{Test}::m::Name", valueType: "@{Test}::m::Cup" }) {
      nodes {
        name {
          type {
            repr
          }
          data
          bcs
        }
        value {
          ... on MoveObject {
            __typename
          }
          ... on MoveValue {
            __typename
          }
        }
      }
    }
  }
}

//# run-graphql
{
  # should yield the 1 Cup dof with T = u64
  object(address: "@{obj_2_0}") {
    dynamicFieldConnection(filter: { nameType: "@{Test}::m", valueType: "@{Test}::m::Cup<u64>" }) {
      nodes {
        name {
          type {
            repr
          }
          data
          bcs
        }
        value {
          ... on MoveObject {
            __typename
          }
          ... on MoveValue {
            __typename
          }
        }
      }
    }
  }
}

//# run-graphql
{
  # should yield the 1 Cup dof with T = u64
  object(address: "@{obj_2_0}") {
    dynamicFieldConnection(filter: { nameType: "@{Test}::m::Name<u64>", valueType: "@{Test}::m::Cup" }) {
      nodes {
        name {
          type {
            repr
          }
          data
          bcs
        }
        value {
          ... on MoveObject {
            __typename
          }
          ... on MoveValue {
            __typename
          }
        }
      }
    }
  }
}

//# run-graphql
{
  # should yield no results
  object(address: "@{obj_2_0}") {
    dynamicFieldConnection(filter: { nameType: "@{Test}::m::Name<u64>", valueType: "@{Test}::m::Cup<0x1::string::String>" }) {
      nodes {
        name {
          type {
            repr
          }
          data
          bcs
        }
        value {
          ... on MoveObject {
            __typename
          }
          ... on MoveValue {
            __typename
          }
        }
      }
    }
  }
}
