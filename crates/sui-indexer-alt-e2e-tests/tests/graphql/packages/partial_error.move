// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

//# init --protocol-version 70 --accounts A --addresses P=0x0 --simulator

//# publish --upgradeable --sender A
module P::M {
  public struct Foo { value: u64 }
  public fun bar(): u64 { 42 }
}

//# create-checkpoint

//# run-graphql
{ # Query module with valid fields and an error field (invalid pagination).
  package(address: "@{P}") {
    module(name: "M") {
      name
      fileFormatVersion
      # Error field - first and last together is invalid
      functions(first: 1, last: 1) {
        nodes {
          name
        }
      }
    }
  }
}

//# run-graphql
{
  # Test partial error at module level
  # One field succeeds, another has partial error
  package(address: "@{P}") {
    validModule: module(name: "M") {
      name
      fileFormatVersion
    }

    partialErrorModule: module(name: "M") {
      name
      # Error field - first and last together is invalid
      structs(first: 1, last: 1) {
        nodes {
          name
        }
      }
    }
  }
}
