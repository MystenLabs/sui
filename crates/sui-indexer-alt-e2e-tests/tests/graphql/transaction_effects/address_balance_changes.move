// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

// Test send_funds and redeem_funds from sui::balance

//# init --addresses test=0x0 --accounts A B --enable-accumulators --simulator --enable-address-balance-gas-payments

// Send 1000000000 from A to B
//# programmable --sender A --inputs 1000000000 @B
//> 0: SplitCoins(Gas, [Input(0)]);
//> 1: sui::coin::into_balance<sui::sui::SUI>(Result(0));
//> 2: sui::balance::send_funds<sui::sui::SUI>(Result(1), Input(1));

//# create-checkpoint

//# view-object 0,1

//# transfer-object --recipient A --sender B --address-balance-gas true 0,1 --gas-budget 1000000000

//# create-checkpoint

//# run-graphql
{ # Test balance_changes field on address balance transfer
  addressBalanceTransferTransaction: transactionEffects(digest: "@{digest_1}") {
    balanceChanges {
      pageInfo {
        hasNextPage
        hasPreviousPage
      }
      nodes {
        owner {
          address
        }
        coinType { repr }
        amount
      }
    }
  }
}

//# run-graphql
{ # Test balance_changes field on transaction paid by address balance
  addressBalanceGasTransaction: transactionEffects(digest: "@{digest_4}") {
    balanceChanges {
      pageInfo {
        hasNextPage
        hasPreviousPage
      }
      nodes {
        owner {
          address
        }
        coinType { repr }
        amount
      }
    }
  }
}
