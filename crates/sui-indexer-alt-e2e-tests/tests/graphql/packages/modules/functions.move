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
