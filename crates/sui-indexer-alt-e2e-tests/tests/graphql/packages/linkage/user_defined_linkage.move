// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

//# init --protocol-version 70 --accounts A --addresses Dep1=0x0 P1=0x0 --simulator

//# publish --upgradeable --sender A
module Dep1::M1 { }

//# publish --upgradeable --dependencies Dep1 --sender A
#[allow(unused_use)]
module P1::M1 {
    use Dep1::M1;
}

//# create-checkpoint

//# run-graphql
{
  package(address: "@{P1}") {
    linkage {
        originalId
        upgradedId
        version
    }
  }
}