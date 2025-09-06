// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

//# init --protocol-version 70 --accounts A --addresses P0=0x0 P1=0x0 --simulator

//# run-graphql
{
  package(address: "@{P0}") {
    module(name: "option") {
      enum(name: "Option") { ...E }
    }
  }
}

fragment E on MoveEnum {
  name
  abilities
  typeParameters {
    constraints
    isPhantom
  }
  variants {
    name
    fields {
      name
      type {
        repr
        signature
      }
    }
  }
}

//# publish --upgradeable --sender A
module P0::M {
  public enum Status has copy, drop {
    Active,
    Inactive { reason: vector<u8> }
  }
}

//# upgrade --package P0 --upgrade-capability 2,1 --sender A
module P1::M {
  public enum Status has copy, drop {
    Active,
    Inactive { reason: vector<u8> }
  }

  public enum Result<T: drop, E: drop> has drop {
    Ok(T),
    Err(E)
  }

  public enum Complex<T: store> has store {
    Empty,
    Single(T),
    Pair { first: T, second: T }
  }
}

//# create-checkpoint

//# run-graphql
{
  packageVersions(address: "@{P0}") {
    nodes {
      version
      module(name: "M") {
        enums {
          nodes { ...E }
        }
      }
    }
  }
}

fragment E on MoveEnum {
  name
  abilities
  typeParameters {
    constraints
    isPhantom
  }
  variants {
    name
    fields {
      name
      type {
        repr
        signature
      }
    }
  }
}

//# run-graphql --cursors "Complex" "Status"
{
  package(address: "@{P1}") {
    module(name: "M") {
      all: enums(first: 50) { ...E }
      first: enums(first: 2) { ...E }
      last: enums(last: 2) { ...E }

      firstBefore: enums(first: 1, before: "@{cursor_1}") { ...E }
      lastAfter: enums(last: 1, after: "@{cursor_0}") { ...E }

      firstAfter: enums(first: 1, after: "@{cursor_0}") { ...E }
      lastBefore: enums(last: 1, before: "@{cursor_1}") { ...E }

      afterBefore: enums(after: "@{cursor_0}", before: "@{cursor_1}") { ...E }
    }
  }
}

fragment E on MoveEnumConnection {
  pageInfo {
    hasPreviousPage
    hasNextPage
  }
  nodes {
    name
  }
}

//# run-graphql
{
  package(address: "@{P0}") {
    module(name: "M") {
      # Test individual enum access
      status: enum(name: "Status") { ...EnumDetails }
      result: enum(name: "Result") { ...EnumDetails }
      complex: enum(name: "Complex") { ...EnumDetails }

      # Test datatype conversions
      statusDatatype: datatype(name: "Status") {
        name
        asMoveEnum { name }
        asMoveStruct { name }
      }

      resultDatatype: datatype(name: "Result") {
        name
        asMoveEnum { name }
        asMoveStruct { name }
      }
    }
  }
}

fragment EnumDetails on MoveEnum {
  name
  module { name }
  abilities
  typeParameters {
    constraints
    isPhantom
  }
  variants {
    name
    fields {
      name
      type {
        repr
        signature
      }
    }
  }
}
