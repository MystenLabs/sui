// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

//# init --protocol-version 70 --accounts A --addresses P1=0x0 P2=0x0 --simulator

//# publish --upgradeable --sender A
module P1::M {
  public struct S1 { }
}

//# upgrade --package P1 --upgrade-capability 1,1 --sender A
module P2::M {
  public struct S1 { }
  public struct S2 { }
}

//# create-checkpoint

//# run-graphql
{
  package(address: "@{obj_1_0}") {
    typeOrigins {
        module
        struct
        definingId
    }
  }
}