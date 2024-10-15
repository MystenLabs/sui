// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

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

//# init --protocol-version 51 --addresses P0=0x0 --accounts A B --simulator --objects-snapshot-min-checkpoint-lag 7

//# publish --sender A
module P0::fake {
    use sui::coin;

    public struct FAKE has drop {}

    fun init(witness: FAKE, ctx: &mut TxContext){
        let (mut treasury_cap, metadata) = coin::create_currency(
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

//# run-graphql --cursors {"c":2,"t":1,"i":false}
# Emulating viewing transaction blocks at checkpoint 2. Fake coin balance should be 700.
{
  transactionBlocks(first: 1, after: "@{cursor_0}", filter: {sentAddress: "@{A}"}) {
    nodes {
      sender {
        fakeCoinBalance: balance(type: "@{P0}::fake::FAKE") {
          totalBalance
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
      }
    }
  }
}

//# run-graphql --cursors {"c":3,"t":1,"i":false}
# Emulating viewing transaction blocks at checkpoint 3. Fake coin balance should be 500.
{
  transactionBlocks(first: 1, after: "@{cursor_0}", filter: {sentAddress: "@{A}"}) {
    nodes {
      sender {
        fakeCoinBalance: balance(type: "@{P0}::fake::FAKE") {
          totalBalance
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
      }
    }
  }
}

//# run-graphql --cursors {"c":4,"t":1,"i":false}
# Emulating viewing transaction blocks at checkpoint 4. Fake coin balance should be 400.
{
  transactionBlocks(first: 1, after: "@{cursor_0}", filter: {sentAddress: "@{A}"}) {
    nodes {
      sender {
        fakeCoinBalance: balance(type: "@{P0}::fake::FAKE") {
          totalBalance
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
      }
    }
  }
}

//# advance-clock --duration-ns 1

//# create-checkpoint

//# run-graphql --cursors {"c":2,"t":1,"i":false}
# Emulating viewing transaction blocks at checkpoint 2. Fake coin balance should be 700.
{
  transactionBlocks(first: 1, after: "@{cursor_0}", filter: {sentAddress: "@{A}"}) {
    nodes {
      sender {
        fakeCoinBalance: balance(type: "@{P0}::fake::FAKE") {
          totalBalance
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
      }
    }
  }
}

//# run-graphql --cursors {"c":3,"t":1,"i":false}
# Emulating viewing transaction blocks at checkpoint 3. Fake coin balance should be 500.
{
  transactionBlocks(first: 1, after: "@{cursor_0}", filter: {sentAddress: "@{A}"}) {
    nodes {
      sender {
        fakeCoinBalance: balance(type: "@{P0}::fake::FAKE") {
          totalBalance
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
      }
    }
  }
}

//# run-graphql --cursors {"c":4,"t":1,"i":false}
# Emulating viewing transaction blocks at checkpoint 4. Fake coin balance should be 400.
{
  transactionBlocks(first: 1, after: "@{cursor_0}", filter: {sentAddress: "@{A}"}) {
    nodes {
      sender {
        fakeCoinBalance: balance(type: "@{P0}::fake::FAKE") {
          totalBalance
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
      }
    }
  }
}

//# advance-clock --duration-ns 1

//# create-checkpoint

//# advance-clock --duration-ns 1

//# create-checkpoint

//# run-graphql --cursors {"c":2,"t":1,"i":false}
# Outside available range
{
  availableRange {
    first {
      sequenceNumber
    }
    last {
      sequenceNumber
    }
  }
  transactionBlocks(first: 1, after: "@{cursor_0}", filter: {sentAddress: "@{A}"}) {
    nodes {
      sender {
        fakeCoinBalance: balance(type: "@{P0}::fake::FAKE") {
          totalBalance
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
      }
    }
  }
}

//# run-graphql --cursors {"c":3,"t":1,"i":false}
# Emulating viewing transaction blocks at checkpoint 3. Fake coin balance should be 500.
{
  transactionBlocks(first: 1, after: "@{cursor_0}", filter: {sentAddress: "@{A}"}) {
    nodes {
      sender {
        fakeCoinBalance: balance(type: "@{P0}::fake::FAKE") {
          totalBalance
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
      }
    }
  }
}

//# run-graphql --cursors {"c":4,"t":1,"i":false}
# Emulating viewing transaction blocks at checkpoint 4. Fake coin balance should be 400.
{
  transactionBlocks(first: 1, after: "@{cursor_0}", filter: {sentAddress: "@{A}"}) {
    nodes {
      sender {
        fakeCoinBalance: balance(type: "@{P0}::fake::FAKE") {
          totalBalance
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
      }
    }
  }
}

//# advance-clock --duration-ns 1

//# create-checkpoint

//# advance-clock --duration-ns 1

//# create-checkpoint

//# advance-clock --duration-ns 1

//# create-checkpoint

//# advance-clock --duration-ns 1

//# create-checkpoint

//# run-graphql --cursors {"c":2,"t":1,"i":false}
# Outside available range
{
  availableRange {
    first {
      sequenceNumber
    }
    last {
      sequenceNumber
    }
  }
  transactionBlocks(first: 1, after: "@{cursor_0}", filter: {sentAddress: "@{A}"}) {
    nodes {
      sender {
        fakeCoinBalance: balance(type: "@{P0}::fake::FAKE") {
          totalBalance
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
      }
    }
  }
}

//# run-graphql --cursors {"c":3,"t":1,"i":false}
# Outside available range
{
  availableRange {
    first {
      sequenceNumber
    }
    last {
      sequenceNumber
    }
  }
  transactionBlocks(first: 1, after: "@{cursor_0}", filter: {sentAddress: "@{A}"}) {
    nodes {
      sender {
        fakeCoinBalance: balance(type: "@{P0}::fake::FAKE") {
          totalBalance
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
      }
    }
  }
}

//# run-graphql --cursors {"c":4,"t":1,"i":false}
# Outside available range
{
  availableRange {
    first {
      sequenceNumber
    }
    last {
      sequenceNumber
    }
  }
  transactionBlocks(first: 1, after: "@{cursor_0}", filter: {sentAddress: "@{A}"}) {
    nodes {
      sender {
        fakeCoinBalance: balance(type: "@{P0}::fake::FAKE") {
          totalBalance
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
      }
    }
  }
}
