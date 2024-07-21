// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

//# init --protocol-version 51 --simulator

//# run-graphql
{
  availableRange {
    first {
      digest
      sequenceNumber
    }
    last {
      digest
      sequenceNumber
    }
  }

  first: checkpoint(id: { sequenceNumber: 0 } ) {
    digest
    sequenceNumber
  }

  last: checkpoint {
    digest
    sequenceNumber
  }
}

//# create-checkpoint


//# create-checkpoint


//# run-graphql
{
  availableRange {
    first {
      digest
      sequenceNumber
    }
    last {
      digest
      sequenceNumber
    }
  }

  first: checkpoint(id: { sequenceNumber: 0 } ) {
    digest
    sequenceNumber
  }

  last: checkpoint {
    digest
    sequenceNumber
  }
}
