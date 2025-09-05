// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

//# init --protocol-version 70 --accounts A B --addresses test=0x0 --simulator

//# publish
module test::dependency_test {
    public struct TestObject has key, store {
        id: UID,
        value: u64,
    }

    public fun create_object(value: u64, ctx: &mut TxContext): TestObject {
        TestObject {
            id: object::new(ctx),
            value,
        }
    }

    public fun object_value(obj: &TestObject): u64 {
        obj.value
    }
}

// Transaction 1: Create an object
// Dependencies: publish transaction, system transactions
//# programmable --sender A --inputs 100u64 @A
//> 0: test::dependency_test::create_object(Input(0));
//> TransferObjects([Result(0)], Input(1))

// Transaction 2: Use objects created in previous transactions
// Dependencies: Transaction 1 (uses object(2,0)), system transactions
//# programmable --sender A --inputs object(2,0)
//> 0: test::dependency_test::object_value(Input(0));

//# create-checkpoint

//# run-graphql
{ # Test dependencies with no dependencies
  genesisTransaction: transactions(first: 1) {
    nodes {
      digest
        effects {
          dependencies {
            pageInfo {
              hasNextPage
              hasPreviousPage
            }
            nodes {
              digest
            }
          }
        }
      }
  }
}

//# run-graphql
{ # Test dependencies field on first user transaction (depends on system transactions)
  systemTransactionDependencies: transactionEffects(digest: "@{digest_2}") {
    digest
    dependencies {
      pageInfo {
        hasNextPage
        hasPreviousPage
      }
      nodes {
        digest
        sender {
          address
        }
      }
    }
  }
}

//# run-graphql
{ # Test dependencies field on second user transaction (depends on both previous user transactions + system transactions)
  userTransactionDependencies: transactionEffects(digest: "@{digest_3}") {
    digest
    dependencies {
      pageInfo {
        hasNextPage
        hasPreviousPage
      }
      nodes {
        digest
        sender {
          address
        }
        gasInput {
          gasBudget
          gasPrice
        }
        kind {
          __typename
        }
      }
    }
  }
}

//# run-graphql
{ # Test pagination functionality with multiple dependencies
  paginationTest: transactionEffects(digest: "@{digest_3}") {
    digest
    dependencies(first: 1) {
      pageInfo {
        hasNextPage
        hasPreviousPage
      }
      edges {
        cursor
        node {
          digest
          sender {
            address
          }
        }
      }
    }
  }

  backwardPaginationTest: transactionEffects(digest: "@{digest_3}") {
    digest
    dependencies(last: 1) {
      pageInfo {
        hasNextPage
        hasPreviousPage
      }
      edges {
        cursor
        node {
          digest
          sender {
            address
          }
        }
      }
    }
  }
}
