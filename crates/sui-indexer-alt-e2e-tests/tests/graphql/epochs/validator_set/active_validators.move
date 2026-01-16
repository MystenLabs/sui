// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

//# init --protocol-version 70 --accounts A --simulator

//# run-graphql
{
  epoch(epochId: 0) {
    epochId
    validatorSet {
      activeValidators {
        pageInfo {
         hasPreviousPage
         hasNextPage
         startCursor
         endCursor
        }

        nodes {
          contents {
            json

            address: extract(path: "metadata.sui_address") {
              asAddress {
                balances {
                  nodes {
                    coinType { repr }
                    totalBalance
                  }
                }

                objects {
                  nodes {
                    contents {
                      type { repr }
                      json
                    }
                  }
                }
              }
            }
          }

          atRisk
        }
      }
    }
  }
}
