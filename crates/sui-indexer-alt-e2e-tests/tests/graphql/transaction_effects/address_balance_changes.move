// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

// Test send_funds and redeem_funds from sui::balance

//# init --protocol-version 106 --addresses test=0x0 --accounts A B C --enable-accumulators --simulator --enable-address-balance-gas-payments

// Send 1000000000 from A to B and A to C
//# programmable --sender A --inputs 1000000000 @B @C
//> 0: SplitCoins(Gas, [Input(0)]);
//> 1: sui::coin::into_balance<sui::sui::SUI>(Result(0));
//> 2: sui::balance::send_funds<sui::sui::SUI>(Result(1), Input(1));
//> 3: SplitCoins(Gas, [Input(0)]);
//> 4: sui::coin::into_balance<sui::sui::SUI>(Result(3));
//> 5: sui::balance::send_funds<sui::sui::SUI>(Result(4), Input(2));

//# create-checkpoint

//# view-object 0,1

// Use address balance as gas
//# transfer-object --recipient A --sender B 0,1 --gas-budget-from-address-balance 1000000000

//# create-checkpoint

// Now have B send address balance to C using address balance as gas
//# programmable --sender B --inputs withdraw<sui::balance::Balance<sui::sui::SUI>>(5000000) @C --gas-budget-from-address-balance 1000000000
//> 0: sui::balance::redeem_funds<sui::sui::SUI>(Input(0));
//> 1: sui::balance::send_funds<sui::sui::SUI>(Result(0), Input(1));

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

//# run-graphql
{ # Test balance_changes field on ab transfer transaction paid by address balance
  addressBalanceGasTransaction: transactionEffects(digest: "@{digest_6}") {
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
