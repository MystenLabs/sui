// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

//# init --protocol-version 51 --addresses P0=0x0 --accounts A --simulator

// Split off a gas coin, so we have an object to query
//# programmable --sender A --inputs 1000 @A
//> 0: SplitCoins(Gas, [Input(0)]);
//> TransferObjects([Result(0)], Input(1))

//# create-checkpoint

//# run-graphql
{
  sender: owner(address: "@{A}") {
    asObject { digest }
  }

  coin: owner(address: "@{obj_1_0}") {
    asObject { digest }
  }
}
