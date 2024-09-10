// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

// Test that available range is correctly updated per objects_snapshot catching up

//# init --protocol-version 51 --simulator --objects-snapshot-min-checkpoint-lag 2

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

//# advance-clock --duration-ns 1

//# create-checkpoint

//# run-graphql
{
  availableRange {
    first {
      sequenceNumber
    }
    last {
      sequenceNumber
    }
  }
}

//# advance-clock --duration-ns 1

//# create-checkpoint

//# run-graphql
{
  availableRange {
    first {
      sequenceNumber
    }
    last {
      sequenceNumber
    }
  }
}

//# advance-clock --duration-ns 1

//# create-checkpoint

//# run-graphql
{
  availableRange {
    first {
      sequenceNumber
    }
    last {
      sequenceNumber
    }
  }
}

//# advance-clock --duration-ns 1

//# create-checkpoint

//# run-graphql
{
  availableRange {
    first {
      sequenceNumber
    }
    last {
      sequenceNumber
    }
  }
}

//# advance-clock --duration-ns 1

//# create-checkpoint

//# run-graphql
{
  availableRange {
    first {
      sequenceNumber
    }
    last {
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
