// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

//# init --protocol-version 70 --accounts A --addresses P=0x0 --simulator

//# programmable --sender A --inputs 1
//> SplitCoins(Gas, [Input(0)]);
//> MergeCoins(Gas, [Result(0)])

//# create-checkpoint

//# run-graphql
{ # Fetch an object and convert it to a MoveObject. Note that the `objectBcs`
  # and `moveObjectBcs` are different.

  object(address: "@{obj_0_0}") {
    objectBcs
    asMoveObject {
      objectBcs
      moveObjectBcs
    }
  }
}

//# run-graphql
{ # Fetch a package, and try to convert it to a MoveObject, which will fail.
  object(address: "0x2") {
    asMoveObject {
      moveObjectBcs
    }
  }
}
