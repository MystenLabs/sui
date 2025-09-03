// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

//# init --protocol-version 70 --accounts A B --addresses test=0x0 --simulator

//# programmable --sender A --inputs 100 @B
//> 0: SplitCoins(Gas, [Input(0)]);
//> 1: TransferObjects([Result(0)], Input(1))

//# publish
module test::gas_test {
    public entry fun simple_function() {
        // Simple function that does minimal work for predictable gas costs
    }
}

//# run test::gas_test::simple_function --sender A

//# create-checkpoint

// Test system transaction created by advance-clock
//# advance-clock --duration-ns 1000000

//# create-checkpoint

//# run-graphql
{ # Test gas_effects field with gasObject and gasSummary
  transferTransaction: transactionEffects(digest: "@{digest_1}") {
    gasEffects {
      gasObject {
        address
        version
        digest
      }
      gasSummary {
        computationCost
        storageCost
        storageRebate
        nonRefundableStorageFee
      }
    }
  }
}

//# run-graphql
{ # Test gas_effects on function call transaction
  functionCallTransaction: transactionEffects(digest: "@{digest_3}") {
    gasEffects {
      gasObject {
        address
        version
        digest
      }
      gasSummary {
        computationCost
        storageCost
        storageRebate
        nonRefundableStorageFee
      }
    }
  }
} 

//# run-graphql
{ # Test gas_effects on system transaction
  functionCallTransaction: transactionEffects(digest: "@{digest_5}") {
    gasEffects {
      gasObject {
        address
        version
        digest
      }
      gasSummary {
        computationCost
        storageCost
        storageRebate
        nonRefundableStorageFee
      }
    }
  }
} 
