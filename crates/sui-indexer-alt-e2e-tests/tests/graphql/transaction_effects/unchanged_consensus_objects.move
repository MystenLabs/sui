// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

// TODO(DVX-1168): Support tests for ConsensusStreamEnded, Cancelled and PerEpochConfig
//# init --protocol-version 70 --accounts A B --addresses test=0x0 --simulator

//# publish
module test::shared_object_tests {
    /// A simple shared object for testing
    public struct SharedCounter has key {
        id: UID,
        value: u64,
    }

    /// Initialize function to create a shared counter
    fun init(ctx: &mut TxContext) {
        let counter = SharedCounter {
            id: object::new(ctx),
            value: 0,
        };
        transfer::share_object(counter);
    }

    /// Read the counter value without modifying it (read-only access)
    public fun get_value(counter: &SharedCounter): u64 {
        counter.value
    }

    /// Increment the counter (mutable access)
    public fun increment(counter: &mut SharedCounter) {
        counter.value = counter.value + 1;
    }
}

//# view-object 1,0

//# programmable --inputs immshared(1,0)
//> 0: test::shared_object_tests::get_value(Input(0))

//# programmable --inputs object(1,0)
//> 0: test::shared_object_tests::increment(Input(0))

//# programmable --inputs immshared(1,0)
//> 0: test::shared_object_tests::get_value(Input(0))

//# create-checkpoint

//# run-graphql
{
  # Test read-only access to consensus object (should show ConsensusObjectRead)
  readOnlyAccess1: transactionEffects(digest: "@{digest_3}") {
    unchangedConsensusObjects {
      edges {
        node {
          ... on ConsensusObjectRead {
            object {
              address
              version
              digest
            }
          }
        }
      }
    }
  }
}

//# run-graphql
{
  # Test mutable access to consensus object (should have no unchanged consensus objects)
  mutableAccess: transactionEffects(digest: "@{digest_4}") {
    unchangedConsensusObjects {
      edges {
        node {
          ... on ConsensusObjectRead {
            object {
              address
              version
            }
          }
        }
      }
    }
  }
}

//# run-graphql
{
  # Test read-only access after mutation (should show ConsensusObjectRead with updated version)
  readOnlyAccess2: transactionEffects(digest: "@{digest_5}") {
    unchangedConsensusObjects {
      edges {
        node {
          ... on ConsensusObjectRead {
            object {
              address
              version
              digest
            }
          }
        }
      }
    }
  }
}
