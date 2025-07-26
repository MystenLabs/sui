// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

//# init --protocol-version 70 --accounts A --simulator

//# run-graphql
{ # todo create test with safeMode enabled
  e0: epoch(epochId: 0) {
    safeMode {
      enabled
      gasSummary {
        computationCost
        storageCost
        storageRebate
        nonRefundableStorageFee
      }
    }
  }
}
