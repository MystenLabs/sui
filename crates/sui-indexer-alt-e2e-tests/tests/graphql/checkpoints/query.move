// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

//# init --protocol-version 70 --accounts A --simulator

//# programmable --sender A --inputs 42 @A
//> 0: SplitCoins(Gas, [Input(0)]);
//> 1: TransferObjects([Result(0)], Input(1))

//# create-checkpoint

//# programmable --sender A --inputs object(1,0) 1
//> 0: SplitCoins(Input(0), [Input(1)]);
//> 1: MergeCoins(Gas, [Result(0)])

//# programmable --sender A --inputs object(1,0) 2
//> 0: SplitCoins(Input(0), [Input(1)]);
//> 1: MergeCoins(Gas, [Result(0)])

//# create-checkpoint

//# create-checkpoint

//# run-graphql
{ # Fetch the latest known checkpoint and version of the object
  ...State

  # Same query at the genesis checkpoint (object should not exist)
  genesis: checkpoint(sequenceNumber: 0) { query { ...State } }

  # ...again after the object was created
  created: checkpoint(sequenceNumber: 1) { query { ...State } }

  # ...again after the object was modified multiple times
  modified: checkpoint(sequenceNumber: 2) { query { ...State } }

  # ...finally after the object was left untouched.
  untouched: checkpoint(sequenceNumber: 3) { query { ...State } }

  # This checkpoint doesn't exist, so it shouldn't be possible to time-travel
  # to it
  nonexistent: checkpoint(sequenceNumber: 4) { query { ...State } }
}

fragment State on Query {
  checkpoint { sequenceNumber }
  object(address: "@{obj_1_0}") { version }
}

//# run-graphql
{ # Querying at a checkpoint hides objects that exist, but at a future
  # checkpoint.
  checkpoint(sequenceNumber: 1) {
    query {
      # Latest as of checkpoint 1
      latest: object(address: "@{obj_1_0}") { version }

      # This version does not exist, so should not return anything
      byVersion: object(address: "@{obj_1_0}", version: 4) { version }

      # "atCheckpoint" will override the fact that this field is nested inside
      # a `Checkpoint.query`.
      atCheckpoint: object(address: "@{obj_1_0}", atCheckpoint: 4) { version }
    }
  }

}
