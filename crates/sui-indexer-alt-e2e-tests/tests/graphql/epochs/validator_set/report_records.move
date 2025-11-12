// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

//# init --protocol-version 70 --accounts A --simulator --num-custom-validator-accounts 2

//# run-graphql
{
  epoch(epochId: 0) {
    epochId
    validatorSet {
      activeValidators {
        nodes {
          name
          # todo DVX-1697 populate reportRecords
          reportRecords {
            nodes {
              name
            }
          }
        }
      }
    }
  }
}
