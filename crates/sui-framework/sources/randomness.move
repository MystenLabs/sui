// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

module sui::randomness {
    use std::hash::sha3_256;
    use std::option::{Self, Option};
    use sui::object::{Self, UID, ID, id};
    use sui::tx_context::{Self, TxContext};

    /// Set is called with an invalid signature.
    const EInvalidSignature: u64 = 0;
    /// Already set object cannot be set again.
    const EAlreadySetObject: u64 = 1;
    /// Unset object cannot be consumed.
    const EUnsetObject: u64 = 2;

    /// Randomness objects can only be created, set or consumed.
    ///
    /// - On creation it contains the epoch in which it was created and a unique id.
    /// - After the object creation transaction is committed, anyone can retrieve the BLS signature on the message
    ///   "randomness:id:epoch" from validators (signed by the key of that epoch).
    /// - The owner of the object can *set* the randomness of the object by supplying the above signature. This
    ///   operation verifies the signature and sets the value of the randomness object to be the hash of the signature.
    /// - The randomness value can be used/consumed.
    ///
    /// Can work both as a shared object and as owned.
    /// - As a shared object - contracts should use the getters to fetch the result.
    /// - As an owned object - contracts should use the getters to check the state of the object, and consume it once
    ///   ready.
    ///
    struct Randomness<phantom T> has key {
        id: UID,
        epoch: u64,
        value: Option<vector<u8>>
    }

    // Q: how can we store associated data? we can use another object and store ids (see below), but can we do better?

    fun new<T: drop>(_w: T, ctx: &mut TxContext): Randomness<T> {
        Randomness {
            id: object::new(ctx),
            epoch: tx_context::epoch(ctx),
            value: option::none(),
        }
    }

    /// Create a new Randomness object and transfer it to the recipient address.
    public fun create_and_transfer<T: drop>(w: T, recipient: address, ctx: &mut TxContext): ID {
        let r = new(w, ctx);
        let id = id(&r);
        sui::transfer::transfer(r, recipient);
        id
    }

    /// Create a new Randomness object and make it a shared object.
    public fun create_as_shared<T: drop>(w: T, ctx: &mut TxContext): ID {
        let r = new(w, ctx);
        let id = id(&r);
        sui::transfer::share_object(r);
        id
    }

    /// Owner(s) can use this function for setting the randomness.
    entry fun set<T>(self: &mut Randomness<T>, sig: vector<u8>) {
        assert!(option::is_none(&self.value), EAlreadySetObject);
        // TODO: construct 'msg'
        // Q: how to convert int to string?
        // // TODO: next api is not available yet.
        // assert!(verify_tbls_signature(self.epoch, msg, sig), EInvalidSignature);
        let hashed = sha3_256(sig);
        self.value = option::some(hashed);
    }

    /// Delete the object and retrieve the randomness (in case of an owned object).
    public fun consume<T: drop>(_w: T, self: Randomness<T>): vector<u8> {
        let Randomness { id, epoch: _, value } = self;
        object::delete(id);
        assert!(option::is_some(&value), EUnsetObject);
        option::extract(&mut value)
    }

    /// Read the epoch of the object.
    public fun epoch<T>(self: &Randomness<T>): u64 {
        self.epoch
    }

    /// Read the current value of the object.
    public fun value<T>(self: &Randomness<T>): &Option<vector<u8>> {
        &self.value
    }
}

//////////////////////////////////////////////////////////////////////
// Examples //

//
module sui::lottery_shared_pool {
    use std::vector;
    use sui::balance::{Self, Balance, zero};
    use sui::coin::{Self, Coin};
    use sui::object::{Self, UID};
    use sui::randomness::{Self, Randomness};
    use sui::sui::SUI;
    use sui::tx_context::{Self, TxContext};

    // Make sure only the current module can access Randomness it creates.
    struct LOTTERY_LOCK has drop {}

    /// Shared object
    struct Lottery has key {
        id: UID,
        balance: Balance<SUI>,
    }

    entry fun create(ctx: &mut TxContext) {
        let lottery = Lottery {
            id: object::new(ctx),
            balance: zero()
        };
        sui::transfer::share_object(lottery);
    }

    // TODO: Currently you can use a ticket on any lottery, not only the one you bought ticket for.
    entry fun buy_ticket(lottery: &mut Lottery, coin: Coin<SUI>, ctx: &mut TxContext) {
        assert!(coin::value(&coin) == 1, 0);
        balance::join(&mut lottery.balance, coin::into_balance(coin));
        randomness::create_and_transfer(LOTTERY_LOCK {}, tx_context::sender(ctx), ctx);
    }

    entry fun use_ticket(lottery: &mut Lottery, ticket: Randomness<LOTTERY_LOCK>, ctx: &mut TxContext) {
        let random_bytes = randomness::consume(LOTTERY_LOCK {}, ticket);
        let first_byte = vector::borrow(&random_bytes, 0);
        if (*first_byte % 100 == 0) {
            let coin = coin::from_balance(balance::split(&mut lottery.balance, 100), ctx);
            sui::pay::keep(coin, ctx);
        }
    }
}

///////////////////////////////////////////////////////


module sui::lottery_owned {
    use std::option;
    use std::vector;
    use sui::balance::{Self, Balance};
    use sui::coin::{Self, Coin};
    use sui::object::{Self, UID, ID, id};
    use sui::randomness::Randomness;
    use sui::randomness;
    use sui::sui::SUI;
    use sui::tx_context::{Self, TxContext};

    // Make sure only the current module can access Randomness it creates.
    struct LOTTERY_LOCK has drop {}

    /// Shared object
    struct Lottery has key {
        id: UID,
        balance: Balance<SUI>,
        creator: address,
    }

    struct Ticket has key {
        id: UID,
        lottery_id: ID,
        creator: address,
        randomness_id: ID,
    }

    entry fun create(coin: Coin<SUI>, ctx: &mut TxContext) {
        let lottery = Lottery {
            id: object::new(ctx),
            balance: coin::into_balance(coin),
            creator: tx_context::sender(ctx),

        };
        sui::transfer::share_object(lottery);
    }

    public fun balance(lottery: &Lottery): u64 {
        balance::value(&lottery.balance)
    }

    public fun creator(lottery: &Lottery): address {
        lottery.creator
    }

    // Buyer gets a randomness object and a ticket that associates the lottery with the randomness, and makes sure that
    // the creator received the payment.
    entry fun buy_ticket(lottery_id: ID, creator: address, coin: Coin<SUI>, ctx: &mut TxContext) {
        assert!(coin::value(&coin) == 1, 0);
        sui::transfer::transfer(coin, creator);
        let randomness_id = randomness::create_and_transfer(LOTTERY_LOCK {}, tx_context::sender(ctx), ctx);
        let ticket = Ticket {
            id: object::new(ctx),
            lottery_id,
            creator,
            randomness_id,
        };
        sui::transfer::transfer(ticket, tx_context::sender(ctx));
    }

    public fun is_winner(lottery: &Lottery, ticket_r: &Randomness<LOTTERY_LOCK>, ticket: &Ticket): bool {
        // Check consistency...
        assert!(id(ticket_r) == ticket.randomness_id, 3);
        assert!(id(lottery) == ticket.randomness_id, 4);
        assert!(lottery.creator == ticket.creator, 5);
        // Check the ticket.
        let random_bytes = randomness::value(ticket_r);
        if (option::is_none(random_bytes)) {
            return false
        };
        let random_bytes = option::borrow(random_bytes);
        let first_byte = vector::borrow(random_bytes, 0);
        *first_byte % 100 == 0
    }

    // Can be called also after all the reward was taken.
    entry fun use_ticket(lottery: &mut Lottery, ticket_r: Randomness<LOTTERY_LOCK>, ticket: &Ticket, ctx: &mut TxContext) {
        if (is_winner(lottery, &ticket_r, ticket)) {
            let coin = coin::from_balance(balance::split(&mut lottery.balance, 100), ctx);
            sui::pay::keep(coin, ctx);
        };
        randomness::consume(LOTTERY_LOCK {}, ticket_r);
    }
}



////////////////////////////////////

module sui::lottery_shared_pool2 {
    use std::vector;
    use sui::balance::{Self, Balance, zero};
    use sui::coin::{Self, Coin};
    use sui::object::{Self, UID, ID, id};
    use sui::randomness::{Self, Randomness};
    use sui::sui::SUI;
    use sui::tx_context::{Self, TxContext};
    use std::option::Option;
    use std::option;

    // Make sure only the current module can access Randomness it creates.
    struct LOTTERY_LOCK has drop {}

    /// Shared object
    struct Lottery has key {
        id: UID,
        balance: Balance<SUI>,
        participants: u8,
        randomness_id: Option<ID>,
    }

    struct Ticket has key {
        id: UID,
        lottery_id: ID,
        participant_id: u8,
    }

    entry fun create(ctx: &mut TxContext) {
        let lottery = Lottery {
            id: object::new(ctx),
            balance: zero(),
            participants: 0,
            randomness_id: option::none(),
        };
        sui::transfer::share_object(lottery);
    }

    entry fun buy_ticket(lottery: &mut Lottery, coin: Coin<SUI>, ctx: &mut TxContext) {
        assert!(lottery.participants < 100, 0); // just to simply the modulo below
        assert!(coin::value(&coin) == 1, 0);
        balance::join(&mut lottery.balance, coin::into_balance(coin));
        let ticket = Ticket {
            id: object::new(ctx),
            lottery_id: id(lottery),
            participant_id: lottery.participants,
        };
        lottery.participants = lottery.participants + 1;
        sui::transfer::transfer(ticket, tx_context::sender(ctx));
    }

    // Create a randomness that will determine the winner.
    entry fun close(lottery: &mut Lottery, ctx: &mut TxContext) {
        assert!(option::is_none(&lottery.randomness_id), 10);
        let randomness_id = randomness::create_as_shared(LOTTERY_LOCK {}, ctx);
        lottery.randomness_id = option::some(randomness_id);
    }

    entry fun use_ticket(lottery: &mut Lottery, randomness: &Randomness<LOTTERY_LOCK>, ticket: Ticket, ctx: &mut TxContext) {
        assert!(option::is_some(randomness::value(randomness)), 11);
        assert!(option::is_some(&lottery.randomness_id), 12);
        assert!(*option::borrow(&lottery.randomness_id) == id(randomness), 13);
        let random_bytes = option::borrow(randomness::value(randomness));
        let first_byte = vector::borrow(random_bytes, 0);
        if (*first_byte % lottery.participants == ticket.participant_id) {
            let amount = balance::value(&lottery.balance);
            let coin = coin::from_balance(balance::split(&mut lottery.balance, amount), ctx);
            sui::pay::keep(coin, ctx);
        };
        let Ticket { id, lottery_id:_, participant_id:_  } = ticket;
        object::delete(id);
    }
}