// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

<<<<<<< HEAD
// cp | coins
// ------------
// 1  | (300, 100, 200) = 600
// 2  | (400, 100, 200) = 700
// 3  | (400, 100)      = 500
// 4  | (400)           = 400
// 7  | (400)

// snapshot@[0, 2), all transaction blocks will have valid data.
// snapshot@[0, 3), first transaction block is out of available range.
// snapshot@[0, 4), first two transaction blocks are out of available range.
// snapshot@[0, 6), all transaction blocks are out of available range.
=======
// Create three coins of a fake currency. Merge the first one with another coin, and hold on to that
// coin. Transfer the rest out to another address B. Validate that we get the expected balances
// across each of 4 transaction blocks. Increment objects_snapshot, verifying that less and less
// data is returned once the data lies beyond the available range. The coin with value 400 should
// continue to show up, even as we move beyond checkpoint 4. Finally, verify that the data is not
// actually gone - the transferred coins should show up under address B.
>>>>>>> 12927087aa (Add historical context to consistent reads)

//# init --addresses P0=0x0 --accounts A B --simulator

//# publish --sender A
module P0::fake {
    use std::option;
    use sui::coin;
    use sui::transfer;
    use sui::tx_context::{Self, TxContext};

    struct FAKE has drop {}

    fun init(witness: FAKE, ctx: &mut TxContext){
        let (treasury_cap, metadata) = coin::create_currency(
            witness,
            2,
            b"FAKE",
            b"",
            b"",
            option::none(),
            ctx,
        );

        let c1 = coin::mint(&mut treasury_cap, 100, ctx);
        let c2 = coin::mint(&mut treasury_cap, 200, ctx);
        let c3 = coin::mint(&mut treasury_cap, 300, ctx);

        transfer::public_freeze_object(metadata);
        transfer::public_transfer(treasury_cap, tx_context::sender(ctx));
        transfer::public_transfer(c1, tx_context::sender(ctx));
        transfer::public_transfer(c2, tx_context::sender(ctx));
        transfer::public_transfer(c3, tx_context::sender(ctx));
    }
}

//# create-checkpoint

//# programmable --sender A --inputs object(1,5) 100 object(1,1)
//> 0: sui::coin::mint<P0::fake::FAKE>(Input(0), Input(1));
//> MergeCoins(Input(2), [Result(0)]);

//# create-checkpoint

//# transfer-object 1,2 --sender A --recipient B

//# create-checkpoint

//# transfer-object 1,3 --sender A --recipient B

//# advance-clock --duration-ns 1

//# create-checkpoint

//# advance-clock --duration-ns 1

//# create-checkpoint

//# advance-clock --duration-ns 1

//# create-checkpoint

//# advance-clock --duration-ns 1

//# create-checkpoint

//# run-graphql
{
  transactionBlocks(filter: {signAddress: "@{A}"}) {
    nodes {
      sender {
<<<<<<< HEAD
        fakeCoinBalance: balance(type: "@{P0}::fake::FAKE") {
          totalBalance
=======
        coins(type:"@{P0}::fake::FAKE") {
          nodes {
            coinBalance
          }
>>>>>>> 12927087aa (Add historical context to consistent reads)
        }
        allBalances: balances {
          nodes {
            coinType {
              repr
            }
            coinObjectCount
            totalBalance
          }
        }
<<<<<<< HEAD
=======
        firstBalance: balances(first: 1) {
          edges {
            cursor
          }
        }
        lastBalance: balances(last: 1) {
          edges {
            cursor
          }
        }
>>>>>>> 12927087aa (Add historical context to consistent reads)
      }
    }
  }
}


//# force-object-snapshot-catchup --start-cp 0 --end-cp 2

//# run-graphql
{
  transactionBlocks(filter: {signAddress: "@{A}"}) {
    nodes {
      sender {
<<<<<<< HEAD
        fakeCoinBalance: balance(type: "@{P0}::fake::FAKE") {
          totalBalance
=======
        coins(type:"@{P0}::fake::FAKE") {
          nodes {
            coinBalance
          }
>>>>>>> 12927087aa (Add historical context to consistent reads)
        }
        allBalances: balances {
          nodes {
            coinType {
              repr
            }
            coinObjectCount
            totalBalance
          }
        }
<<<<<<< HEAD
=======
        firstBalance: balances(first: 1) {
          edges {
            cursor
          }
        }
        lastBalance: balances(last: 1) {
          edges {
            cursor
          }
        }
>>>>>>> 12927087aa (Add historical context to consistent reads)
      }
    }
  }
}

<<<<<<< HEAD

=======
>>>>>>> 12927087aa (Add historical context to consistent reads)
//# force-object-snapshot-catchup --start-cp 0 --end-cp 3

//# run-graphql
{
  transactionBlocks(filter: {signAddress: "@{A}"}) {
    nodes {
      sender {
<<<<<<< HEAD
        fakeCoinBalance: balance(type: "@{P0}::fake::FAKE") {
          totalBalance
=======
        coins(type:"@{P0}::fake::FAKE") {
          nodes {
            coinBalance
          }
>>>>>>> 12927087aa (Add historical context to consistent reads)
        }
        allBalances: balances {
          nodes {
            coinType {
              repr
            }
            coinObjectCount
            totalBalance
          }
        }
<<<<<<< HEAD
=======
        firstBalance: balances(first: 1) {
          edges {
            cursor
          }
        }
        lastBalance: balances(last: 1) {
          edges {
            cursor
          }
        }
>>>>>>> 12927087aa (Add historical context to consistent reads)
      }
    }
  }
}

<<<<<<< HEAD

=======
>>>>>>> 12927087aa (Add historical context to consistent reads)
//# force-object-snapshot-catchup --start-cp 0 --end-cp 4

//# run-graphql
{
  transactionBlocks(filter: {signAddress: "@{A}"}) {
    nodes {
      sender {
<<<<<<< HEAD
        fakeCoinBalance: balance(type: "@{P0}::fake::FAKE") {
          totalBalance
=======
        coins(type:"@{P0}::fake::FAKE") {
          nodes {
            coinBalance
          }
>>>>>>> 12927087aa (Add historical context to consistent reads)
        }
        allBalances: balances {
          nodes {
            coinType {
              repr
            }
            coinObjectCount
            totalBalance
          }
        }
<<<<<<< HEAD
=======
        firstBalance: balances(first: 1) {
          edges {
            cursor
          }
        }
        lastBalance: balances(last: 1) {
          edges {
            cursor
          }
        }
>>>>>>> 12927087aa (Add historical context to consistent reads)
      }
    }
  }
}


//# force-object-snapshot-catchup --start-cp 0 --end-cp 6

//# run-graphql
<<<<<<< HEAD
=======
# We should still see the fake coin with value 400.
>>>>>>> 12927087aa (Add historical context to consistent reads)
{
  transactionBlocks(filter: {signAddress: "@{A}"}) {
    nodes {
      sender {
<<<<<<< HEAD
        fakeCoinBalance: balance(type: "@{P0}::fake::FAKE") {
          totalBalance
=======
        coins(type:"@{P0}::fake::FAKE") {
          nodes {
            coinBalance
          }
>>>>>>> 12927087aa (Add historical context to consistent reads)
        }
        allBalances: balances {
          nodes {
            coinType {
              repr
            }
            coinObjectCount
            totalBalance
          }
        }
<<<<<<< HEAD
=======
        firstBalance: balances(first: 1) {
          edges {
            cursor
          }
        }
        lastBalance: balances(last: 1) {
          edges {
            cursor
          }
        }
      }
    }
  }
}

//# run-graphql
{
  address(address: "@{B}") {
    coins(type: "@{P0}::fake::FAKE") {
      nodes {
        coinBalance
      }
    }
    allBalances: balances {
      nodes {
        coinType {
          repr
        }
        coinObjectCount
        totalBalance
      }
    }
    firstBalance: balances(first: 1) {
      edges {
        cursor
      }
    }
    lastBalance: balances(last: 1) {
      edges {
        cursor
>>>>>>> 12927087aa (Add historical context to consistent reads)
      }
    }
  }
}
