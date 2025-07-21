// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

//# init --protocol-version 70 --accounts A --simulator

//# advance-clock --duration-ns 123000000

//# advance-epoch

//# advance-clock --duration-ns 321000000

//# advance-epoch

//# advance-epoch

//# run-graphql
{
  latest: epoch { ...E }

  e0: epoch(epochId: 0) { ...E }
  e1: epoch(epochId: 1) { ...E }
  e2: epoch(epochId: 2) { ...E }

  # This epoch doesn't exist yet
  e3: epoch(epochId: 3) { ...E }
}

fragment E on Epoch {
  epochId
  referenceGasPrice
  startTimestamp
  endTimestamp
  totalCheckpoints
  totalGasFees
}

//# run-graphql
{ # This checkpoint is half way through an earlier epoch, which should be
  # reflected in the latest epoch we get a start time for and an end time for.
  checkpoint(sequenceNumber: 2) {
    query {
      latest: epoch { ...E }
      e0: epoch(epochId: 0) { ...E }
      e1: epoch(epochId: 1) { ...E }
      e2: epoch(epochId: 2) { ...E }
    }
  }
}

fragment E on Epoch {
  epochId
  referenceGasPrice
  startTimestamp
  endTimestamp
  totalCheckpoints
  totalGasFees
}
