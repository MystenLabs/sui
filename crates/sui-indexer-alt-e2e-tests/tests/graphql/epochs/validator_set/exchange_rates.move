// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

//# init --protocol-version 70 --accounts A --simulator

//# run-graphql
{
  epoch0: epoch(epochId: 0) { ...E }
  epoch1: epoch(epochId: 1) { ...E }
}

fragment E on Epoch {
  epochId
  validatorSet {
    activeValidators {
      nodes {
        contents {
          name: format(format: "{metadata.name}")
          exchangeRates: extract(path: "staking_pool.exchange_rates.id") {
            asAddress {
              dynamicFields {
                nodes {
                  name {
                    json
                  }
                  value {
                    ... on MoveValue {
                      json
                    }
                  }
                }
              }
            }
          }
        }
      }
    }
  }
}

//# advance-epoch

//# create-checkpoint

//# run-graphql
{
  epoch0: epoch(epochId: 0) { ...E }
  epoch1: epoch(epochId: 1) { ...E }
}

fragment E on Epoch {
  epochId
  validatorSet {
    activeValidators {
      nodes {
        contents {
          name: format(format: "{metadata.name}")
          exchangeRates: extract(path: "staking_pool.exchange_rates.id") {
            asAddress {
              dynamicFields {
                nodes {
                  name {
                    json
                  }
                  value {
                    ... on MoveValue {
                      json
                    }
                  }
                }
              }
            }
          }
        }
      }
    }
  }
}

//# run-graphql
{ # addressAt can be used to view a wrapped object at the latest checkpoint.
  epoch(epochId: 0) {
    validatorSet {
      activeValidators {
        nodes {
          contents {
            exchangeRates: extract(path: "staking_pool.exchange_rates.id") {
              asAddress {
                addressAt {
                  dynamicFields {
                    nodes {
                      name { json }
                    }
                  }
                }
              }
            }
          }
        }
      }
    }
  }
}
