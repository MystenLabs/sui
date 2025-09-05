// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

//# init --protocol-version 70 --accounts A --addresses P0=0x0 P1=0x0 --simulator

//# run-graphql
{
  package(address: "0x2") {
    coin: module(name: "coin") {
      struct(name: "Coin") { ...S }
    }

    tx_context: module(name: "tx_context") {
      struct(name: "TxContext") { ...S }
    }
  }
}

fragment S on MoveStruct {
  name
  abilities
  typeParameters {
    constraints
    isPhantom
  }
  fields {
    name
    type {
      repr
      signature
    }
  }
}

//# publish --upgradeable --sender A
module P0::M {
  public struct S has copy, drop { x: u64 }
}

//# upgrade --package P0 --upgrade-capability 2,1 --sender A
module P1::M {
  public struct S has copy, drop { x: u64 }
  public struct T<U: drop> { y: u64, s: S, u: U }
  public struct V { t: T<S> }
}

//# create-checkpoint

//# run-graphql
{
  packageVersions(address: "@{P0}") {
    nodes {
      version
      module(name: "M") {
        structs {
          nodes { ...S }
        }
      }
    }
  }
}

fragment S on MoveStruct {
  name
  abilities
  typeParameters {
    constraints
    isPhantom
  }
  fields {
    name
    type {
      repr
      signature
    }
  }
}

//# run-graphql --cursors "Coin" "TreasuryCap"
{
  package(address: "0x2") {
    module(name: "coin") {
      all: structs(first: 50) { ...S }
      first: structs(first: 3) { ...S }
      last: structs(last: 3) { ...S }

      firstBefore: structs(first: 3, before: "@{cursor_1}") { ...S }
      lastAfter: structs(last: 3, after: "@{cursor_0}") { ...S }

      firstAfter: structs(first: 3, after: "@{cursor_0}") { ...S }
      lastBefore: structs(last: 3, before: "@{cursor_1}") { ...S }

      afterBefore: structs(after: "@{cursor_0}", before: "@{cursor_1}") { ...S }
    }
  }
}

fragment S on MoveStructConnection {
  pageInfo {
    hasPreviousPage
    hasNextPage
  }
  nodes {
    name
  }
}
