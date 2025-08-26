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

//# advance-epoch

//# programmable --sender A --inputs object(1,0) 1
//> 0: SplitCoins(Input(0), [Input(1)]);
//> 1: MergeCoins(Gas, [Result(0)])

//# programmable --sender A --inputs object(1,0) 2
//> 0: SplitCoins(Input(0), [Input(1)]);
//> 1: MergeCoins(Gas, [Result(0)])

//# create-checkpoint

//# programmable --sender A --inputs object(1,0) 3
//> 0: SplitCoins(Input(0), [Input(1)]);
//> 1: MergeCoins(Gas, [Result(0)])

//# create-checkpoint

//# create-checkpoint

//# run-graphql
{
  allCheckpoints: checkpoints(first: 10) {
    edges {
      node { ...CheckpointFields }
    }
  }
  checkpointsAtEpoch0AfterCheckpoint1: checkpoints(first: 5, filter: {atEpoch: 0, afterCheckpoint: 1}) {
    edges {
      node { ...CheckpointFields }
    }
  }
  checkpointsAtEpoch0BeforeCheckpoint2: checkpoints(first: 5, filter: {atEpoch: 0, beforeCheckpoint: 2}) {
    edges {
      node { ...CheckpointFields }
    }
  }
  checkpointsAtEpoch1AfterCheckpoint2BeforeCheckpoint5: checkpoints(
    first: 10, 
    filter: {
      atEpoch: 1, 
      afterCheckpoint: 2, 
      beforeCheckpoint: 5
    }
  ) {
    edges { node { ...CheckpointFields } }
  }
  checkpointsAtEpoch1AtCheckpoint4AfterCheckpoint2BeforeCheckpoint6: checkpoints(
    first: 10,
    filter: {
      atEpoch: 1,
      afterCheckpoint: 2,
      beforeCheckpoint: 6,
      atCheckpoint: 4
    }
  ) {
    edges { node { ...CheckpointFields } }
  }
  checkpointsAtEpoch1BeforeCheckpoint1NotInEpochIsNone: checkpoints(first: 5, filter: {atEpoch: 1, beforeCheckpoint: 1}) {
    edges {
      node { ...CheckpointFields }
    }
  }
  checkpointsAtNonExistentEpoch5IsNone: checkpoints(first: 10, filter: {atEpoch: 5}) {
    edges {
      node { ...CheckpointFields }
    }
  }
  # Test at_checkpoint filter on a checkpoint not in the epoch.
  checkpointsAtEpoch1AtCheckpoint3AfterCheckpoint2BeforeCheckpoint5IsNone: checkpoints(
    first: 10,
    filter: {
      atEpoch: 1,
      afterCheckpoint: 2,
      beforeCheckpoint: 5,
      atCheckpoint: 3
    }
  ) {
    edges { node { ...CheckpointFields } }
  }
}

fragment CheckpointFields on Checkpoint {
  sequenceNumber
  digest
  epoch {epochId}
}