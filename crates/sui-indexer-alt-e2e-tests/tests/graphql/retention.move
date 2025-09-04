// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

//# init --protocol-version 70 --accounts A --simulator

//# create-checkpoint

//# create-checkpoint

//# advance-clock --duration-ns 321000000

//# create-checkpoint

//# run-graphql
{
  queryTransactionsRetention: retention(type: "Query", field: "transactions", filter: "") {
    ...RetentionFragment
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
  queryInvalidRetention: retention(type: "Query", field: "invalid", filter: "") {
    first {
      sequenceNumber
    }
    last {
      sequenceNumber
    }
  }
}