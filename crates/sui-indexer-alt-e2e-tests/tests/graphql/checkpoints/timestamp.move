// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

//# init --protocol-version 70 --accounts A --simulator

//# advance-clock --duration-ns 123000000

//# create-checkpoint

//# advance-clock --duration-ns 321000000

//# create-checkpoint

//# run-graphql
{ # Fetch each checkpoint individually, and then in a multi-get
  c0: checkpoint(sequenceNumber: 0) { ...Cp }
  c1: checkpoint(sequenceNumber: 1) { ...Cp }
  c2: checkpoint(sequenceNumber: 2) { ...Cp }
  multiGetCheckpoints(keys: [0, 1, 2]) { ...Cp }
}

fragment Cp on Checkpoint {
  sequenceNumber
  timestamp
}

//# run-graphql
{ # Fetch a non-existent checkpoint
  checkpoint(sequenceNumber: 3) {
    sequenceNumber
  }
}

//# run-graphql
{ # Multi-get a mix of existing and non-existing checkpoints
  multiGetCheckpoints(keys: [2, 100, 0, 200]) {
    sequenceNumber
  }
}
