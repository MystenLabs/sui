// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

//# init --protocol-version 51 --simulator --objects-snapshot-min-checkpoint-lag 2

//# run-graphql
{
  happyPath: serviceConfig {
    availableRange(type: "Query", field: "checkpoints") {
      first {
        digest
        sequenceNumber
      }
      last {
        digest
        sequenceNumber
      }
    }
  }
}

//# run-graphql
{
  typeNotFound: serviceConfig {
    availableRange(type: "InvalidType", field: "checkpoints") {
      first {
        digest
        sequenceNumber
      }
    }
  }
}

//# run-graphql
{
  fieldNotFound: serviceConfig {
    availableRange(type: "Query", field: "invalidField") {
      first {
        digest
        sequenceNumber
      }
    }
  }
}

//# run-graphql
{
  invalidTypeAndField: serviceConfig {
    availableRange(type: "InvalidType", field: "invalidField") {
      first {
        digest
        sequenceNumber
      }
    }
  }
}

//# run-graphql
{
  notAnObjectOrInterface: serviceConfig {
    availableRange(type: "ZkLoginIntentScope", field: "invalidField") {
      first {
        digest
        sequenceNumber
      }
    }
  }
}