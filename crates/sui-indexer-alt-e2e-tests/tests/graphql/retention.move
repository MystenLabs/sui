// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

//# init --protocol-version 70 --accounts A --simulator --objects-snapshot-min-checkpoint-lag 2

//# programmable --sender A --inputs 1000 @A
//> 0: SplitCoins(Gas, [Input(0)]);
//> 1: TransferObjects([Result(0)], Input(1))

//# create-checkpoint

//# create-checkpoint

//# create-checkpoint

//# run-graphql
{
  serviceConfig {
    transactionQuery: retention(type: "Query", field: "transaction") {
    ...RetentionFragment
    }
    consistentQuery: retention(type: "Address", field: "balances") {
    ...RetentionFragment
    }
  }
}

fragment RetentionFragment on AvailableRange {
  first {
    sequenceNumber
  }
  last {
    sequenceNumber
  }
}  

//# run-graphql
{
  serviceConfig {
    retention(type: "Query", field: "invalid") {
      first {
        sequenceNumber
      }
      last {
        sequenceNumber
      }
    }
  }
}