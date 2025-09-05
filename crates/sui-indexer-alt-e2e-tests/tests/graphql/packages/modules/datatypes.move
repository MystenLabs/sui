// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

//# init --protocol-version 70 --accounts A --addresses P=0x0 --simulator

//# publish
module P::M {
  use sui::coin::Coin;
  use sui::sui::SUI;

  public struct S has copy, drop { x: u64 }
  public enum E has store {
    F(u8, u16),
    G { y: address }
  }

  public struct T<phantom U: drop> {
    z: u32,
    w: vector<Coin<SUI>>,
  }

  public enum V<W: key> {
    X { a: W },
    Y(vector<W>),
    Z,
  }

  public struct A() has copy, drop, store;
}

//# create-checkpoint

//# run-graphql
{
  package(address: "@{P}") {
    module(name: "M") {
      A: datatype(name: "A") { ...D }
      E: datatype(name: "E") { ...D }
      S: datatype(name: "S") { ...D }
      T: datatype(name: "T") { ...D }
      V: datatype(name: "V") { ...D }

      doesntExist: datatype(name: "DoesntExist") { ...D }

      datatypes {
        nodes { ...D }
      }
    }
  }
}

fragment D on MoveDatatype {
  name
  abilities
  typeParameters {
    constraints
    isPhantom
  }

  asMoveStruct { name }
}

//# run-graphql --cursors "A" "V"
{
  package(address: "@{P}") {
    module(name: "M") {
      all: datatypes(first: 10) { ...D }

      first: datatypes(first: 3) { ...D }
      last: datatypes(last: 3) { ...D }

      firstBefore: datatypes(first: 3, before: "@{cursor_1}") { ...D }
      lastAfter: datatypes(last: 3, after: "@{cursor_0}") { ...D }

      firstAfter: datatypes(first: 3, after: "@{cursor_0}") { ...D }
      lastBefore: datatypes(last: 3, before: "@{cursor_1}") { ...D }

      afterBefore: datatypes(after: "@{cursor_0}", before: "@{cursor_1}") { ...D }
    }
  }
}


fragment D on MoveDatatypeConnection {
  pageInfo {
    hasPreviousPage
    hasNextPage
  }
  nodes {
    name
  }
}
