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
}

//# run-graphql
{ # This checkpoint is half way through the epoch, so the epoch should return
  # its starting information, but not its ending information.
  checkpoint(sequenceNumber: 2) {
    query {
      epoch(epochId: 1) {
        epochId
        referenceGasPrice
        startTimestamp
        endTimestamp
      }
    }
  }
}
