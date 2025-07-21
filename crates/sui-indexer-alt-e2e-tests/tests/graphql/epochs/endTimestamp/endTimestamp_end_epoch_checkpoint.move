// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

//# init --protocol-version 70 --accounts A --simulator

//# advance-epoch

//# run-graphql
{
  checkpoint(sequenceNumber: 1) {
    query {
      epoch(epochId: 0) {
        startTimestamp
        endTimestamp
      }
    }
  }
}
