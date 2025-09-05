// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

//# init --protocol-version 70 --accounts A --addresses P1=0x0 P2=0x0  P3=0x0 --simulator

//# publish --upgradeable --sender A
module P1::M {
  public fun foo(): u64 { 42 }
}

//# upgrade --package P1 --upgrade-capability 1,1 --sender A
module P2::M {
  public fun foo(): u64 { 43 }
}

//# create-checkpoint

//# upgrade --package P2 --upgrade-capability 1,1 --sender A
module P3::M {
  public fun foo(): u64 { 44 }
}

//# create-checkpoint

//# run-graphql
{ # Fetching packages as objects, to confirm their addresses and versions
  p1: object(address: "@{obj_1_0}") {
    address
    version
  }

  p2: object(address: "@{obj_2_0}") {
    address
    version
  }

  p3: object(address: "@{obj_4_0}") {
    address
    version
  }
}

//# run-graphql
{
  package(address: "@{obj_1_0}") {
    address
    version

    initial: packageAt(version: 1) {
      address
      version
    }

    byCheckpoint: packageAt(checkpoint: 1) {
      address
      version
    }
  }
}
