// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

//# init --protocol-version 70 --accounts A --addresses Dep1=0x0 P1=0x0 Dep2=0x0 P2=0x0 --simulator

//# publish --upgradeable --sender A
module Dep1::M1 { }

//# publish --upgradeable --dependencies Dep1 --sender A
#[allow(unused_use)]
module P1::M1 {
    use Dep1::M1;
}

//# create-checkpoint

//# publish --upgradeable --sender A
module Dep2::M1 { }

//# upgrade --package P1 --upgrade-capability 2,1 --dependencies Dep2 --sender A
#[allow(unused_use)]
module P2::M1 {
    use Dep2::M1;
}

//# create-checkpoint

//# run-graphql
{
  package(address: "@{P2}") {
    version
    linkage {
        originalId
        upgradedId
        version
    }
  }
}