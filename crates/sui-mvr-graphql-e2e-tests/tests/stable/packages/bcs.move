// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

//# init --protocol-version 51 --addresses P=0x0 --simulator

//# publish
module P::m {
  public fun f(): u64 { 42 }
}

//# create-checkpoint

//# run-graphql
{
  package(address: "@{P}") {
    bcs
    packageBcs
    moduleBcs
  }
}
