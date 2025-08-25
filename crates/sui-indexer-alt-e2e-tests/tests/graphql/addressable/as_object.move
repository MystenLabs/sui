// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

//# init --protocol-version 70 --accounts A --simulator

//# programmable --sender A --inputs 1000 @A
//> 0: SplitCoins(Gas, [Input(0)]);
//> 1: TransferObjects([Result(0)], Input(1))

//# programmable --sender A --inputs 2000 @A
//> 0: SplitCoins(Gas, [Input(0)]);
//> 1: TransferObjects([Result(0)], Input(1))

//# create-checkpoint

//# programmable --sender A --inputs 3000 @A
//> 0: SplitCoins(Gas, [Input(0)]);
//> 1: TransferObjects([Result(0)], Input(1))

//# create-checkpoint

//# run-graphql
{
  objectVersions(address: "@{obj_0_0}") {
    nodes { version}
  }

  latest: address(address: "@{obj_0_0}") {
    asObject { version }
  }

  cp1: checkpoint(sequenceNumber: 1) {
    query {
      address(address: "@{obj_0_0}") { asObject { version } }
    }
  }

  versionBounded: address(address: "@{obj_0_0}", rootVersion: 2) {
    asObject { version }
  }
}
