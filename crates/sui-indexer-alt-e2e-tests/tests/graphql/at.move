// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

//# init --protocol-version 70 --addresses P=0x0 --simulator --accounts A

//# programmable --sender A --inputs 1000u64 @A
//> 0: SplitCoins(Gas, [Input(0)]);
//> 1: TransferObjects([Result(0)], Input(1))

//# create-checkpoint

//# programmable --sender A --inputs object(1,0) 1u64 @A
//> 0: SplitCoins(Gas, [Input(0)]);
//> 1: TransferObjects([Result(0)], Input(1))

//# programmable --sender A --inputs object(2,0) 2u64 @A
//> 0: SplitCoins(Gas, [Input(0)]);
//> 1: TransferObjects([Result(0)], Input(1))

//# create-checkpoint

//# programmable --sender A --inputs object(3,0) 3u64 @A
//> 0: SplitCoins(Gas, [Input(0)]);
//> 1: TransferObjects([Result(0)], Input(1))

//# create-checkpoint

//# run-graphql
{
  # Query APIs always default to the viewed checkpoint
  object(address: "@{obj_1_0}") { version }
  address(address: "@{obj_1_0}") { asObject { version } }

  # The viewed checkpoint is changed by `Checkpoint.query`, while `*At` queries
  # override that.
  c2: checkpoint(sequenceNumber: 1) {
    query {
      object(address: "@{obj_1_0}") {
        version
        latestObject: objectAt { version }
      }

      address(address: "@{obj_1_0}") {
        asObject { version }
        latestAddress: addressAt { asObject { version } }
      }
    }
  }
}
