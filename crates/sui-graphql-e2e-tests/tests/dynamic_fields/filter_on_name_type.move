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
  object(address: "@{obj_2_0}") {
    dynamicFieldConnection {
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
  object(address: "@{obj_2_0}") {
    dynamicFieldConnection(filter: { nameType: "@{Test}" }) {
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
  object(address: "@{obj_2_0}") {
    dynamicFieldConnection(filter: { nameType: "@{Test}::m" }) {
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
  # should be an empty result, as the type of name is u64
  object(address: "@{obj_2_0}") {
    dynamicFieldConnection(filter: { nameType: "@{Test}::m::Child" }) {
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
  # should yield a df and the Child dof
  object(address: "@{obj_2_0}") {
    dynamicFieldConnection(filter: { nameType: "u64" }) {
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
  object(address: "@{obj_2_0}") {
    dynamicFieldConnection(filter: { nameType: "@{Test}::m::Name" }) {
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
  object(address: "@{obj_2_0}") {
    dynamicFieldConnection(filter: { nameType: "@{Test}::m::Name<u64>" }) {
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
  object(address: "@{obj_2_0}") {
    dynamicFieldConnection(filter: { nameType: "@{Test}::m::Name<0x1::string::String>" }) {
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
  # should yield an empty result as the name type of Cup is Name
  object(address: "@{obj_2_0}") {
    dynamicFieldConnection(filter: { nameType: "@{Test}::m::Cup" }) {
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
  # should yield an empty result as the name type of Cup is Name
  object(address: "@{obj_2_0}") {
    dynamicFieldConnection(filter: { nameType: "@{Test}::m::Cup<u64>" }) {
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
  # should yield an empty result as the name type of Cup is Name
  object(address: "@{obj_2_0}") {
    dynamicFieldConnection(filter: { nameType: "@{Test}::m::Cup<0x1::string::String>" }) {
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
  object(address: "@{obj_2_0}") {
    dynamicFieldConnection(filter: { nameType: "vector<u8>" }) {
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
  # should yield the single bool df
  object(address: "@{obj_2_0}") {
    dynamicFieldConnection(filter: { nameType: "bool" }) {
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
  # should not yield any results - TODO: decide if this is the expected result
  object(address: "@{obj_2_0}") {
    dynamicFieldConnection(filter: { nameType: "vector" }) {
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
