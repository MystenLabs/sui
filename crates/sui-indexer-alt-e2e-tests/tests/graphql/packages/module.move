// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

//# init --protocol-version 70 --accounts A --addresses P=0x0 Q=0x0 --simulator

//# publish --upgradeable --sender A
module P::M {
  public fun foo(): u64 { 42 }
}

module P::N {
  public fun bar(): u64 { 43 }
}

//# create-checkpoint

//# upgrade --package P --upgrade-capability 1,1 --sender A
module P::M {
  public fun foo(): u64 { 42 }
}

module P::N {
  public fun bar(): u64 { 43 }
}

module P::O {
  public fun baz(): u64 { 44 }
}

//# create-checkpoint

//# run-graphql
{
  cp1: checkpoint(sequenceNumber: 1) {
    query {
      package(address: "@{P}") {
        m: module(name: "M") {
          name
          fileFormatVersion
          bytes
        }

        n: module(name: "N") {
          name
          fileFormatVersion
          bytes
        }

        o: module(name: "O") {
          name
          fileFormatVersion
          bytes
        }
      }
    }
  }

  cp2: checkpoint(sequenceNumber: 2) {
    query {
      package(address: "@{P}") {
        m: module(name: "M") {
          name
          fileFormatVersion
          bytes
        }

        n: module(name: "N") {
          name
          fileFormatVersion
          bytes
        }

        o: module(name: "O") {
          name
          fileFormatVersion
          bytes
        }
      }
    }
  }
}
