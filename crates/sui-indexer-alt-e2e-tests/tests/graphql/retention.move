// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

//# init --protocol-version 70 --accounts A --simulator --objects-snapshot-min-checkpoint-lag 2

//# programmable --sender A --inputs 1000 @A
//> 0: SplitCoins(Gas, [Input(0)]);
//> 1: TransferObjects([Result(0)], Input(1))

//# run-graphql
{ # Start of network, only Cp 0 is available
  serviceConfig {
    retention(type: "Address", field: "asObject") {
      first {
        sequenceNumber
      }
      last {
        sequenceNumber
      }
    }
  }
}

//# create-checkpoint

//# run-graphql
{ # Two Checkpoints, Cp 0 and Cp 1 are available, after next checkpoint is created we expect Cp0 to not be in available range for queries that are backed by obj_versions
  serviceConfig {
    retention(type: "Address", field: "asObject") {
      first {
        sequenceNumber
      }
      last {
        sequenceNumber
      }
    }
  }
}

//# create-checkpoint

//# run-graphql
{ # Queries that are backed by obj_versions should only have retention of 2 latest Cps
  objectVersionsPruned: serviceConfig {
    retention(type: "Address", field: "asObject") {
      first {
        sequenceNumber
      }
      last {
        sequenceNumber
      }
    }
  }
}

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