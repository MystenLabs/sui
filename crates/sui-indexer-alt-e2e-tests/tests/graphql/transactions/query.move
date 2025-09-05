// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

//# init --protocol-version 70 --addresses Test=0x0 --accounts A --simulator

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

//# run-graphql
# When a range is not specified, transactions queries should return results starting from the smallest unpruned tx
{
  transactions(first: 10) {
    pageInfo {
      startCursor
      endCursor
      hasPreviousPage
      hasNextPage
    }
    edges {
      cursor
      node {
        digest
        effects {
            checkpoint {
                sequenceNumber
                digest
            }
        }
      }
    }
  }
}

//# run-graphql --cursors 0 3
# Offset from the back, select so the page is full
{
  transactions(first: 5, before: "@{cursor_1}") {
    pageInfo {
      startCursor
      endCursor
      hasPreviousPage
      hasNextPage
    }
    edges {
      cursor
      node {
        digest
        effects {
            checkpoint {
                sequenceNumber
                digest
            }
        }
      }
    }
  }
}

//# run-graphql --cursors 0 4
# Offset from front and back, select two from the front so has_next_page and has_previous_page are true
{
  transactions(after: "@{cursor_0}", first: 2, before: "@{cursor_1}") {
    pageInfo {
      startCursor
      endCursor
      hasPreviousPage
      hasNextPage
    }
    edges {
      cursor
      node {
        digest
        effects {
            checkpoint {
                sequenceNumber
                digest
            }
        }
      }
    }
  }
}

//# run-graphql --cursors 0 6
# Offset from front and back, select two from the front so has_next_page and has_previous_page are true
{
  transactions(after: "@{cursor_0}", last: 2, before: "@{cursor_1}") {
    pageInfo {
      startCursor
      endCursor
      hasPreviousPage
      hasNextPage
    }
    edges {
      cursor
      node {
        digest
        effects {
            checkpoint {
                sequenceNumber
                digest
            }
        }
      }
    }
  }
}

//# run-graphql --cursors 0
# Offset from front paginate backwards
{
  transactions(after: "@{cursor_0}", last: 2) {
    pageInfo {
      startCursor
      endCursor
      hasPreviousPage
      hasNextPage
    }
    edges {
      cursor
      node {
        digest
        effects {
            checkpoint {
                sequenceNumber
                digest
            }
        }
      }
    }
  }
}
