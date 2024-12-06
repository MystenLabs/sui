// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

//# init --protocol-version 51 --addresses Test=0x0 --accounts A --simulator --epochs-to-keep 2

//# publish
module Test::M1 {
    use sui::coin::Coin;

    public struct Object has key, store {
        id: UID,
        value: u64,
    }

    fun foo<T: key, T2: drop>(_p1: u64, value1: T, _value2: &Coin<T2>, _p2: u64): T {
        value1
    }

    public entry fun create(value: u64, recipient: address, ctx: &mut TxContext) {
        transfer::public_transfer(
            Object { id: object::new(ctx), value },
            recipient
        )
    }
}

//# run Test::M1::create --sender A --args 0 @A

//# create-checkpoint

//# advance-epoch

//# run Test::M1::create --sender A --args 1 @A

//# create-checkpoint

//# advance-epoch

//# run Test::M1::create --sender A --args 2 @A

//# create-checkpoint

//# advance-epoch

//# run Test::M1::create --sender A --args 3 @A

//# create-checkpoint

//# run-graphql --wait-for-checkpoint-pruned 4
# The smallest unpruned epoch is 2, starting with checkpoint sequence number 5
# When a range is not specified, transactions queries should return results starting from the smallest unpruned tx
{
  epoch {
    epochId
  }
  checkpoints {
    nodes {
      epoch {
        epochId
      }
      sequenceNumber
    }
  }
  unfiltered: transactionBlocks {
    pageInfo {
      startCursor
      endCursor
      hasPreviousPage
      hasNextPage
    }
    nodes {
      digest
      effects {
        checkpoint {
          sequenceNumber
        }
      }
    }
  }
  transactionBlocks(filter: { sentAddress: "@{A}" }) {
    pageInfo {
      startCursor
      endCursor
      hasPreviousPage
      hasNextPage
    }
    nodes {
      digest
      effects {
        checkpoint {
          sequenceNumber
        }
      }
    }
  }
}

//# run-graphql --wait-for-checkpoint-pruned 4
# In the absence of an upper bound, graphql sets the upper bound to `checkpoint_viewed_at`'s max tx + 1 (right-open interval)
{
  transactionBlocks(filter: { afterCheckpoint: 5 sentAddress: "@{A}" }) {
    pageInfo {
      startCursor
      endCursor
      hasPreviousPage
      hasNextPage
    }
    nodes {
      digest
      effects {
        checkpoint {
          sequenceNumber
        }
      }
    }
  }
}

//# run-graphql --wait-for-checkpoint-pruned 4
# In the absence of a lower bound, graphql sets the lower bound to the smallest unpruned checkpoint's min tx
{
  transactionBlocks(filter: { beforeCheckpoint: 7 sentAddress: "@{A}" }) {
    pageInfo {
      startCursor
      endCursor
      hasPreviousPage
      hasNextPage
    }
    nodes {
      digest
      effects {
        checkpoint {
          sequenceNumber
        }
      }
    }
  }
}



//# run-graphql --wait-for-checkpoint-pruned 4
# If the caller tries to fetch data outside of the unpruned range, they should receive an empty connection.
{
  transactionBlocks(filter: { atCheckpoint: 0 sentAddress: "@{A}" }) {
    pageInfo {
      startCursor
      endCursor
      hasPreviousPage
      hasNextPage
    }
    nodes {
      digest
      effects {
        checkpoint {
          sequenceNumber
        }
      }
    }
  }
}

//# run-graphql --wait-for-checkpoint-pruned 4
# Empty response if caller tries to fetch data beyond the available upper bound
{
  transactionBlocks(filter: { atCheckpoint: 10 sentAddress: "@{A}" }) {
    pageInfo {
      startCursor
      endCursor
      hasPreviousPage
      hasNextPage
    }
    nodes {
      digest
      effects {
        checkpoint {
          sequenceNumber
        }
      }
    }
  }
}


//# run-graphql
# Mirror from the back
{
  transactionBlocks(last: 10 filter: { sentAddress: "@{A}" }) {
    pageInfo {
      startCursor
      endCursor
      hasPreviousPage
      hasNextPage
    }
    nodes {
      digest
      effects {
        checkpoint {
          sequenceNumber
        }
      }
    }
  }
}

//# run-graphql
# Mirror from the back
{
  transactionBlocks(last: 10 filter: { afterCheckpoint: 5 sentAddress: "@{A}" }) {
    pageInfo {
      startCursor
      endCursor
      hasPreviousPage
      hasNextPage
    }
    nodes {
      digest
      effects {
        checkpoint {
          sequenceNumber
        }
      }
    }
  }
}

//# run-graphql
# Mirror from the back
{
  transactionBlocks(last: 10 filter: { beforeCheckpoint: 7 sentAddress: "@{A}" }) {
    pageInfo {
      startCursor
      endCursor
      hasPreviousPage
      hasNextPage
    }
    nodes {
      digest
      effects {
        checkpoint {
          sequenceNumber
        }
      }
    }
  }
}

//# run-graphql --cursors {"c":7,"t":6,"i":false}
# The first tx after pruning has seq num 6.
# When viewed at checkpoint 7, there are two more txs that follow it.
{
  transactionBlocks(after: "@{cursor_0}") {
    pageInfo {
      startCursor
      endCursor
      hasPreviousPage
      hasNextPage
    }
    nodes {
      digest
      effects {
        checkpoint {
          sequenceNumber
        }
      }
    }
  }
}

//# run-graphql --cursors {"c":7,"t":0,"i":false}
# Data is pruned and no longer available
{
  transactionBlocks(after: "@{cursor_0}") {
    pageInfo {
      startCursor
      endCursor
      hasPreviousPage
      hasNextPage
    }
    nodes {
      digest
      effects {
        checkpoint {
          sequenceNumber
        }
      }
    }
  }
}

//# run-graphql --cursors {"c":7,"t":0,"i":true}
# Data is pruned and no longer available
{
  transactionBlocks(after: "@{cursor_0}") {
    pageInfo {
      startCursor
      endCursor
      hasPreviousPage
      hasNextPage
    }
    nodes {
      digest
      effects {
        checkpoint {
          sequenceNumber
        }
      }
    }
  }
}

//# run-graphql --cursors {"c":7,"t":0,"i":true}
# Data is pruned and no longer available
{
  transactionBlocks(scanLimit: 10000 after: "@{cursor_0}") {
    pageInfo {
      startCursor
      endCursor
      hasPreviousPage
      hasNextPage
    }
    nodes {
      digest
      effects {
        checkpoint {
          sequenceNumber
        }
      }
    }
  }
}
