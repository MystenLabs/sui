// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

//# init --protocol-version 70 --accounts A --simulator

//# programmable --sender A --inputs 42 @A
//> 0: SplitCoins(Gas, [Input(0)]);
//> 1: TransferObjects([Result(0)], Input(1))

//# create-checkpoint

//# run-graphql
{ # Query epoch with valid fields and an error field (invalid pagination).
  epoch(epochId: 0) {
    epochId
    referenceGasPrice
    # Error field - first and last together is invalid
    checkpoints(first: 1, last: 1) {
      nodes {
        sequenceNumber
      }
    }
  }
}

//# run-graphql
{
  # Test partial error at query root level
  # One epoch query succeeds with all fields, another has partial error
  validEpoch: epoch(epochId: 0) {
    epochId
    referenceGasPrice
  }

  partialErrorEpoch: epoch(epochId: 0) {
    epochId
    # Error field - first and last together is invalid
    transactions(first: 1, last: 1) {
      nodes {
        digest
      }
    }
  }
}
