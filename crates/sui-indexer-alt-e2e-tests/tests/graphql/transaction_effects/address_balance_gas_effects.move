// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

//# init --protocol-version 108 --accounts A --addresses test=0x0 --enable-accumulators --simulator --enable-address-balance-gas-payments

//# programmable --sender A --inputs 1000000000 @A
//> 0: SplitCoins(Gas, [Input(0)]);
//> 1: sui::coin::into_balance<sui::sui::SUI>(Result(0));
//> 2: sui::balance::send_funds<sui::sui::SUI>(Result(1), Input(1));

//# publish
module test::mod {
    public fun fails() {
        abort 42
    }

    public fun succeeds() {
        // do nothing
    }
}

//# create-checkpoint

//# programmable --sender A --address-balance-gas --inputs 1000
//> 0: test::mod::fails()

//# programmable --sender A --address-balance-gas
//> 0: test::mod::succeeds()

//# create-checkpoint

//# run-graphql
{
  failed: transactionEffects(digest: "@{digest_4}") { ...E }
  success: transactionEffects(digest: "@{digest_5}") { ...E }
}

fragment E on TransactionEffects {
  status

  transaction {
    sender { address }
  }

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

  balanceChanges {
    nodes {
      owner { address }
      coinType { repr }
      amount
    }
  }
}
