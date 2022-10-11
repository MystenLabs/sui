// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

module nfts::num {
    use sui::object::{Self, UID};
    use sui::transfer;
    use sui::tx_context::{Self, TxContext};

    /// Very silly NFT: a natural number!
    struct Num has key, store {
        id: UID,
        n: u64
    }

    struct NumIssuerCap has key {
        id: UID,
        /// Number of NFT<Num>'s in circulation. Fluctuates with minting and burning.
        /// A maximum of `MAX_SUPPLY` NFT<Num>'s can exist at a given time.
        supply: u64,
        /// Total number of NFT<Num>'s that have been issued. Always <= `supply`.
        /// The next NFT<Num> to be issued will have the value of the counter.
        issued_counter: u64,
    }

    /// Only allow 10 NFT's to exist at once. Gotta make those NFT's rare!
    const MAX_SUPPLY: u64 = 10;

    /// Created more than the maximum supply of Num NFT's
    const ETooManyNums: u64 = 0;

    /// Create a unique issuer cap and give it to the transaction sender
    fun init(ctx: &mut TxContext) {
        let issuer_cap = NumIssuerCap {
            id: object::new(ctx),
            supply: 0,
            issued_counter: 0,
        };
        transfer::transfer(issuer_cap, tx_context::sender(ctx))
    }

    /// Create a new `Num` NFT. Aborts if `MAX_SUPPLY` NFT's have already been issued
    public fun mint(cap: &mut NumIssuerCap, ctx: &mut TxContext): Num {
        let n = cap.issued_counter;
        cap.issued_counter = n + 1;
        cap.supply = cap.supply + 1;
        assert!(n <= MAX_SUPPLY, ETooManyNums);
        Num { id: object::new(ctx), n }
    }

    /// Burn `nft`. This reduces the supply.
    /// Note: if we burn (e.g.) the NFT<Num> for 7, that means
    /// no Num with the value 7 can exist again! But if the supply
    /// is maxed out, burning will allow us to mint new Num's with
    /// higher values.
    public fun burn(cap: &mut NumIssuerCap, nft: Num) {
        let Num { id, n: _ } = nft;
        cap.supply = cap.supply - 1;
        object::delete(id);
    }
}
