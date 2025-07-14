// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

//# init --protocol-version 70 --accounts A --addresses test=0x0 --simulator

//# publish
module test::simple {
    public struct Counter has key {
        id: UID,
        value: u64,
    }

    fun init(ctx: &mut TxContext) {
        transfer::share_object(Counter {
            id: object::new(ctx),
            value: 0,
        })
    }

    public fun increment(counter: &mut Counter) {
        counter.value = counter.value + 1;
    }
}

// Digest 2: Basic transaction with single gas payment
//# programmable --sender A --inputs object(1,0)
//> 0: test::simple::increment(Input(0));

// Digest 3: Split gas coin into multiple pieces for later use as gas payments
// This creates object(3,0) and object(3,1) which will be used in other digests
//# programmable --sender A --inputs 500000000 300000000 @A
//> 0: SplitCoins(Gas, [Input(0), Input(1)]);
//> 1: TransferObjects([NestedResult(0,0), NestedResult(0,1)], Input(2));

// Digest 4: Use BOTH split coins from Digest 3 as multiple gas payments
//# programmable --sender A --gas-payment 3,0 --gas-payment 3,1 --gas-budget 500000000 --inputs object(1,0)
//> 0: test::simple::increment(Input(0));

//# create-checkpoint

// Test system transaction created by advance-clock
//# advance-clock --duration-ns 1000000

//# create-checkpoint

//# run-graphql
{ # Test basic single gas payment transaction
  basicTransaction: transaction(digest: "@{digest_2}") {
    gasInput {
      gasSponsor {
        address
      }
      gasPrice
      gasBudget
      gasPayment {
        pageInfo {
          hasNextPage
          hasPreviousPage
        }
        nodes {
          address
          version
        }
      }
    }
  }
}

//# run-graphql
{ # Test multiple gas payments using split coins from previous transaction
  multipleGasPaymentsTransaction: transaction(digest: "@{digest_4}") {
    gasInput {
      gasSponsor {
        address
      }
      gasPrice
      gasBudget
      gasPayment {
        pageInfo {
          hasNextPage
          hasPreviousPage
        }
        nodes {
          address
          version
        }
      }
    }
  }
}


//# run-graphql
{ # Test pagination functionality with multiple gas payments
  paginationTest: transaction(digest: "@{digest_4}") {
    gasInput {
      gasPayment(first: 1) {
        pageInfo {
          hasNextPage
          hasPreviousPage
          endCursor
        }
        nodes {
          address
          version
        }
      }
    }
  }
}

//# run-graphql
{ # Test backward pagination functionality with multiple gas payments
  backwardPaginationTest: transaction(digest: "@{digest_4}") {
    gasInput {
      gasPayment(last: 1) {
        pageInfo {
          hasNextPage
          hasPreviousPage
          startCursor
        }
        nodes {
          address
          version
        }
      }
    }
  }
}

//# run-graphql
{ # Test system transaction GasInput
  systemTransaction: transaction(digest: "@{digest_6}") {
    gasInput {
      gasSponsor {
        address
      }
      gasPrice
      gasBudget
      gasPayment {
        pageInfo {
          hasNextPage
          hasPreviousPage
        }
        nodes {
          address
          version
        }
      }
    }
  }
}
