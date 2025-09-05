// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

//# init --protocol-version 70 --accounts A --addresses P=0x0 --simulator

//# publish
module P::M {
  public fun f(): u64 { 42 }
  entry fun g(): u64 { 43 }

  public(package) fun h<T: drop + store, U: copy + drop>(xs: vector<T>, ys: vector<U>): u64 {
    xs.length() + ys.length()
  }

  public fun i(x: u64): (u64, u64) { (x, x + 1) }
}

//# create-checkpoint

//# run-graphql
{
  package(address: "@{P}") {
    module(name: "M") {
      f: function(name: "f") { ...F }
      g: function(name: "g") { ...F }
      h: function(name: "h") { ...F }
      i: function(name: "i") { ...F }
      # Doesn't exist
      x: function(name: "x") { ...F }
    }
  }
}

fragment F on MoveFunction {
  name
  isEntry
  parameters { repr signature }
  return { repr signature }
  typeParameters { constraints }
  visibility
}

//# run-graphql --cursors "f" "i"
{
  package(address: "@{P}") {
    module(name: "M") {
      all: functions(first: 4) { ...F }
      first: functions(first: 2) { ...F }
      last: functions(last: 2) { ...F }

      firstBefore: functions(first: 2, before: "@{cursor_1}") { ...F }
      lastAfter: functions(last: 2, after: "@{cursor_0}") { ...F }

      firstAfter: functions(first: 2, after: "@{cursor_0}") { ...F }
      lastBefore: functions(last: 2, before: "@{cursor_1}") { ...F }

      afterBefore: functions(after: "@{cursor_0}", before: "@{cursor_1}") { ...F }
    }
  }
}

fragment F on MoveFunctionConnection {
  pageInfo {
    hasPreviousPage
    hasNextPage
  }
  nodes {
    name
  }
}
