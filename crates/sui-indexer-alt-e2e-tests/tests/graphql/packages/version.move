// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

//# init --protocol-version 70 --accounts A --addresses P1=0x0 P2=0x0  P3=0x0 --simulator

//# publish --upgradeable --sender A
module P1::M {
  public fun foo(): u64 { 42 }
}

//# create-checkpoint

//# upgrade --package P1 --upgrade-capability 1,1 --sender A
module P2::M {
  public fun foo(): u64 { 43 }
}

//# upgrade --package P2 --upgrade-capability 1,1 --sender A
module P3::M {
  public fun foo(): u64 { 44 }
}

//# programmable --sender A --inputs 42 @A
//> 0: SplitCoins(Gas, [Input(0)]);
//> 1: TransferObjects([Result(0)], Input(1))

//# create-checkpoint

//# run-graphql
{ # Fetching packages as objects, to confirm their addresses and versions
  p1: object(address: "@{obj_1_0}") {
    address
    version
  }

  p2: object(address: "@{obj_3_0}") {
    address
    version
  }

  p3: object(address: "@{obj_4_0}") {
    address
    version
  }
}

//# run-graphql
{ # Look up package versions using the original package address

  # To get the initial version, the version needs to be set explicitly
  p1: package(address: "@{obj_1_0}", version: 1) {
    address
    version
  }

  # Any version is accessible that way
  p2: package(address: "@{obj_1_0}", version: 2) {
    address
    version
  }

  # The latest version can also be accessible by explicitly passing a version
  p3: package(address: "@{obj_1_0}", version: 3) {
    address
    version
  }

  # This version doesn't exist
  p4: package(address: "@{obj_1_0}", version: 4) {
    address
    version
  }
}

//# run-graphql
{ # The ID of any package version works as an anchor
  p1: package(address: "@{obj_4_0}", version: 1) {
    address
    version
  }

  p2: package(address: "@{obj_4_0}", version: 2) {
    address
    version
  }

  p3: package(address: "@{obj_4_0}", version: 3) {
    address
    version
  }

  p4: package(address: "@{obj_4_0}", version: 4) {
    address
    version
  }
}

//# run-graphql
{ # Works for system packages as well
  explicit: package(address: "0x1", version: 1) {
    address
    version
  }

  notThere: package(address: "0x1", version: 2) {
    address
    version
  }
}

//# run-graphql
{ # If the object is not a move package, then there is no response
  package(address: "@{obj_5_0}", version: 5) {
    address
    version
  }

  object(address: "@{obj_5_0}") {
    address
    version
  }
}
