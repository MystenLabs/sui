// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

//# init --simulator

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




// Handle unions specially; use the max
// For connections, recurse. Check if edge or node

// Then we rework arrays

// rollback explain cost & observe: mention in RPC chat to descope

// add devx prog to code owners for graphql