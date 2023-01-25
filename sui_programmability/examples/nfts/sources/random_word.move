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
    use std::option;
    use std::string::String;
    use std::vector;
    use sui::coin::{Self, Coin};
    use sui::object::{Self, UID, ID};
    use sui::randomness;
    use sui::sui::SUI;
    use sui::transfer;
    use sui::tx_context::{sender, TxContext};

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
        let len = vector::length(&weights);
        while (i < len) {
            total_weight = total_weight + (*vector::borrow(&weights, i) as u64);
            i = i + 1;
        };

        let shop = Shop {
            id: object::new(ctx),
            issuer: sender(ctx),
            words,
            weights,
            total_weight,
        };
        transfer::freeze_object(shop);
    }

    /// Buy a random word:
    /// - Pay 1 MIST to to issuer.
    /// - Create a randomness object.
    /// - Create a ticket that references the randomness object.
    /// - Transfer both objects to the caller.
    public entry fun buy(shop: &Shop, coin: Coin<SUI>, ctx: &mut TxContext) {
        assert!(coin::value(&coin) == 1, 0);
        transfer::transfer(coin, shop.issuer);

        let randomness = randomness::new(RANDOMNESS_WITNESS {}, ctx);
        let ticket = Ticket {
            id: object::new(ctx),
            shop_id: object::id(shop),
            randomness_id: object::id(&randomness),
        };
        randomness::transfer(randomness, sender(ctx));
        transfer::transfer(ticket, sender(ctx));
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
        let selection = randomness::safe_selection(shop.total_weight, random_value);

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
                transfer::transfer(word, sender(ctx));
                break
            };
            curr_weight = curr_weight + weight;
            i = i + 1;
        };

        // Release objects.
        randomness::destroy(randomness);
        let Ticket { id, shop_id: _, randomness_id: _ } = ticket;
        object::delete(id);
    }
}
