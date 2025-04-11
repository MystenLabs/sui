// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

//# init --protocol-version 70 --accounts A B --addresses test=0x0 --simulator

//# publish
module test::counter {
  public struct Counter has key {
    id: UID,
    x: u64,
  }

  public struct NFT has key, store {
    id: UID,
    x: u64
  }

  public struct NFTMinted has copy, drop, store {
    id: ID,
  }

  fun init(ctx: &mut TxContext) {
    transfer::share_object(Counter {
        id: object::new(ctx),
        x: 0,
    })
  }

  public fun inc(c: &mut Counter) { c.x = c.x + 1 }
  public fun inc_by(c: &mut Counter, x: u64) { c.x = c.x + x }

  public fun take(c: &mut Counter, x: u64, ctx: &mut TxContext): NFT {
    assert!(c.x >= x);
    c.x = c.x - x;
    let nft = NFT { id: object::new(ctx), x };

    sui::event::emit(NFTMinted { id: object::id(&nft) });
    nft
  }
}

//# programmable --sender A --inputs object(1,0) 42 @A
//> 0: test::counter::inc(Input(0));
//> 1: test::counter::inc_by(Input(0), Input(1));
//> 2: sui::coin::value<sui::sui::SUI>(Gas);
//> 3: test::counter::inc_by(Input(0), Result(2));
//> 4: test::counter::take(Input(0), Input(1));
//> 5: TransferObjects([Result(4)], Input(2))

//# programmable --sender A --inputs 42 @B
//> 0: SplitCoins(Gas, [Input(0)]);
//> 1: TransferObjects([Result(0)], Input(1))

//# create-checkpoint

//# run-graphql
{ # "Not found" cases
  transaction(digest: "11111111111111111111111111111111") {
    transactionBcs
  }

  transactionEffects(digest: "11111111111111111111111111111111") {
    effectsBcs
  }
}

//# run-graphql
{ # Fetching raw transactions and their effects, separately
  transactionA: transaction(digest: "@{digest_2}") {
    transactionBcs
  }

  transactionB: transaction(digest: "@{digest_3}") {
    transactionBcs
  }

  effectsA: transactionEffects(digest: "@{digest_2}") {
    effectsBcs
  }

  effectsB: transactionEffects(digest: "@{digest_3}") {
    effectsBcs
  }
}

//# run-graphql
{ # Fetch raw transactions and their effects, using multi-get
  multiGetTransactions(keys: [
    "@{digest_2}",
    "@{digest_3}"
  ]) {
    transactionBcs
  }

  multiGetTransactionEffects(keys: [
    "@{digest_2}",
    "@{digest_3}"
  ]) {
    effectsBcs
  }
}
