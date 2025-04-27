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
{ # Look up packages at checkpoints

  # Without a filter, the RPC watermark is used.
  latest: package(address: "@{obj_1_0}") {
    address
    version
  }

  # This is from before the first version of the package was published, so it
  # shouldn't be there
  c0: package(address: "@{obj_1_0}", atCheckpoint: 0) {
    address
    version
  }

  # First version
  c1: package(address: "@{obj_1_0}", atCheckpoint: 1) {
    address
    version
  }

  # The package went through two version changes in the next checkpoint
  c2: package(address: "@{obj_1_0}", atCheckpoint: 2) {
    address
    version
  }

  # Checkpoint 3 does not exist, so the package is unchanged at this version.
  c3: package(address: "@{obj_1_0}", atCheckpoint: 3) {
    address
    version
  }
}

//# run-graphql
{ # The ID of any package version works as an anchor

  latest: package(address: "@{obj_3_0}") {
    address
    version
  }

  c0: package(address: "@{obj_3_0}", atCheckpoint: 0) {
    address
    version
  }

  c1: package(address: "@{obj_3_0}", atCheckpoint: 1) {
    address
    version
  }

  c2: package(address: "@{obj_3_0}", atCheckpoint: 2) {
    address
    version
  }

  c3: package(address: "@{obj_3_0}", atCheckpoint: 3) {
    address
    version
  }
}

//# run-graphql
{ # Works for system packages as well
  implicit: package(address: "0x1") {
    address
    version
  }

  c0: package(address: "0x1", atCheckpoint: 0) {
    address
    version
  }

  c1: package(address: "0x1", atCheckpoint: 1) {
    address
    version
  }

  c2: package(address: "0x1", atCheckpoint: 2) {
    address
    version
  }
}

//# run-graphql
{ # If the object is not a move package, then there is no response
  implicit: package(address: "@{obj_5_0}") {
    address
    version
  }

  explicit: package(address: "@{obj_5_0}", atCheckpoint: 2) {
    address
    version
  }

  object(address: "@{obj_5_0}", atCheckpoint: 2) {
    address
    version
  }
}
