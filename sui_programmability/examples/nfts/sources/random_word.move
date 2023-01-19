// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

/// A NFT shop that creates NFTs with random words and can be minted without going through consensus.
/// It is an example of a secure auto generated NFTs factory, e.g. for selling game items.
///
/// The NFT creator defines the possible words and their weights.
/// Anyone can purchase a word from the shop, chosen randomly for the user according to the distribution defined by
/// the weights.
///
module nfts::random_word {
    use sui::object::{Self, UID, ID};
    use sui::tx_context::{Self, TxContext};
    use std::string::String;
    use sui::coin::Coin;
    use sui::sui::SUI;
    use sui::coin;
    use sui::randomness;
    use std::option;
    use std::vector;

    const EWrongShop: u64 = 1;
    const EWrongRandomness: u64 = 2;
    const EInvalidRndLength: u64 = 3;
    const EInvalidSize: u64 = 4;

    /// Instances of this type will be immutable objects, thus users could use them without going through consensus.
    struct Shop has key {
        id: UID,
        issuer: address,
        words: vector<String>,
        weights: vector<u8>,
        total_weight: u64,
    }

    struct Ticket has key {
        id: UID,
        shop_id: ID,
        randomness_id: ID,
    }

    /// The "word NFT" which links to the creating shop and has the random word chosen for the user.
    struct Word has key {
        id: UID,
        word: String,
        creator: ID,
    }

    struct RANDOMNESS_WITNESS has drop {}

    /// Create a new word NFTs shop for a given set of words and their weights.
    public entry fun create(words: vector<String>, weights: vector<u8>, ctx: &mut TxContext) {
        assert!(vector::length(&words) == vector::length(&weights), EInvalidSize);
        let total_weight: u64 = 0;
        let i = 0;
        while (i < vector::length(&weights)) {
            total_weight = total_weight + (*vector::borrow(&weights, i) as u64);
            i = i + 1;
        };

        let shop = Shop {
            id: object::new(ctx),
            issuer: tx_context::sender(ctx),
            words,
            weights,
            total_weight,
        };
        sui::transfer::freeze_object(shop);
    }

    /// Buy a random word:
    /// - Pay 1 SUI to to issuer.
    /// - Create a randomness object.
    /// - Create a ticket that references the randomness object.
    /// - Transfer both objects to the caller.
    public entry fun buy(shop: &Shop, coin: Coin<SUI>, ctx: &mut TxContext) {
        assert!(coin::value(&coin) == 1, 0);
        sui::transfer::transfer(coin, shop.issuer);

        let randomness = randomness::new(RANDOMNESS_WITNESS {}, ctx);
        let ticket = Ticket {
            id: object::new(ctx),
            shop_id: object::id(shop),
            randomness_id: object::id(&randomness),
        };
        randomness::transfer(randomness, tx_context::sender(ctx));
        sui::transfer::transfer(ticket, tx_context::sender(ctx));
    }

    /// Set the Randomness object with the given signature, and create the derived word NFT.
    public entry fun mint(
        shop: &Shop,
        ticket: Ticket,
        randomness: randomness::Randomness<RANDOMNESS_WITNESS>,
        sig: vector<u8>,
        ctx: &mut TxContext
    ) {
        assert!(ticket.shop_id == object::id(shop), EWrongShop);
        assert!(ticket.randomness_id == object::id(&randomness), EWrongRandomness);

        randomness::set(&mut randomness, sig);
        let random_value = option::borrow(randomness::value(&randomness));
        let selection = safe_selection(shop.total_weight, random_value);

        // Find the relevant word by iterating over the weights.
        let curr_weight: u64 = 0;
        let i = 0;
        while (curr_weight < shop.total_weight) {
            let weight = (*vector::borrow(&shop.weights, i) as u64);
            if (selection < curr_weight + weight) {
                let word = Word {
                    id: object::new(ctx),
                    word: *vector::borrow(&shop.words, i),
                    creator: object::id(shop),
                };
                sui::transfer::transfer(word, tx_context::sender(ctx));
                break
            };
            curr_weight = curr_weight + weight;
            i = i + 1;
        };

        // Release objects.
        randomness::destroy(randomness);
        destroy_ticket(ticket);
    }

    fun destroy_ticket(ticket: Ticket) {
        let Ticket { id, shop_id: _, randomness_id: _ } = ticket;
        object::delete(id);
    }

    // Given a vector with uniform random bytes, convert its first 16 bytes to a u128 number and output its modulo
    // with input n. Since n is u64, the output is at most 2^{-64} biased.
    fun safe_selection(n: u64, rnd: &vector<u8>): u64 {
        assert!(vector::length(rnd) >= 16, EInvalidRndLength);
        let m: u128 = 0;
        let i = 0;
        while (i < 16) {
            m = m << 8;
            let curr_byte = *vector::borrow(rnd, i);
            m = m + (curr_byte as u128);
            i = i + 1;
        };
        let n_128 = (n as u128);
        let module_128  = m % n_128;
        let res = (module_128 as u64);
        res
    }
}
