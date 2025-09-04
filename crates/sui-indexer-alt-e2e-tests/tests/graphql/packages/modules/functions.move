// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

//# init --protocol-version 70 --accounts A --addresses P=0x0 --simulator

//# publish
module P::M {
  public fun f(): u64 { 42 }
  entry fun g(): u64 { 43 }
}

//# create-checkpoint

//# run-graphql
{
  package(address: "@{P}") {
    module(name: "M") {
      f: function(name: "f") { ...F }
      g: function(name: "g") { ...F }
      # Doesn't exist
      x: function(name: "x") { ...F }
    }
  }
}

fragment F on MoveFunction {
  name
  isEntry
}
