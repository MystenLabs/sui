// // Copyright (c) Mysten Labs, Inc.
// // SPDX-License-Identifier: Apache-2.0

/// Randomness objects can only be created, set or consumed. They cannot be created and consumed
/// in the same transaction since it might allow validators include creat and use those objects
/// *after* seeing the randomness they depend on.
///
/// - On creation, the object contains the epoch in which it was created and a unique object id.
///
/// - After the object creation transaction is committed, anyone can retrieve the BLS signature on
///   the message "randomness:id:epoch" from validators (signed using the Threshold-BLS key of that
///   epoch).
///
/// - Anyone that can mutate the object can set the randomness of the object by supplying the BLS
///   signature. This operation verifies the signature and sets the value of the randomness object
///   to be the hash of the signature.
///
///   Note that there is exactly one signature that could pass this verification for an object,
///   thus, the only options the owner of the object has after retrieving the signature is to either
///   set the randomness or leave it unset. Applications that use Randomness objects must make sure
///   they handle both options (e.g., debit the user on object creation so even if the user aborts
///   depending on the randomness it received, the application is not harmed).
///
/// - Once set, the actual randomness value can be read/consumed.
///
///
/// This object can be used as a shared-/owned-object.
///
module sui::randomness_keyed {
    use std::hash::sha3_256;
    use std::option::{Self, Option};
    use sui::object::{Self, UID};
    use sui::tx_context::{Self, TxContext};
    use sui::transfer;

    /// Set is called with an invalid signature.
    const EInvalidSignature: u64 = 0;
    /// Already set object cannot be set again.
    const EAlreadySet: u64 = 1;

    struct Randomness<phantom T> has key {
        id: UID,
        epoch: u64,
        value: Option<vector<u8>>
    }

    public fun new<T: drop>(_w: T, ctx: &mut TxContext): Randomness<T> {
        Randomness<T> {
            id: object::new(ctx),
            epoch: tx_context::epoch(ctx),
            value: option::none(),
        }
    }

    public fun transfer<T>(self: Randomness<T>, to: address) {
        transfer::transfer(self, to);
    }

    public fun share_object<T>(self: Randomness<T>) {
        transfer::share_object(self);
    }

    /// Owner(s) can use this function for setting the randomness.
    public fun set<T>(self: &mut Randomness<T>, sig: vector<u8>) {
        assert!(option::is_none(&self.value), EAlreadySet);
        // TODO: Construct 'msg'
        //  msg = "randomness":id:epoch;
        // TODO: Next API is not available yet.
        //  assert!(verify_tbls_signature(self.epoch, msg, sig), EInvalidSignature);
        let hashed = sha3_256(sig);
        self.value = option::some(hashed);
    }

    /// Delete the object and retrieve the randomness (in case of an owned object).
    public fun destroy<T>(r: Randomness<T>) {
        let Randomness { id, epoch: _, value: _ } = r;
        object::delete(id);
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

// TODO: example with deriving longer than 32 bytes


//////////////////////////////////////////////////////////////////////
// Examples //

// Scratchcard that uses a shared obj for the reward pool, and randomness<> as a ticket
module sui::scratchcard_example {
    use std::vector;
    use sui::balance::{Self, Balance, zero};
    use sui::coin::{Self, Coin};
    use sui::object::{Self, UID};
    use sui::randomness_keyed::{Self, Randomness};
    use sui::sui::SUI;
    use sui::tx_context::{Self, TxContext};
    use std::option;

    // Make sure only the current module can access Randomness it creates.
    struct LOTTERY_LOCK has drop {}

    /// Shared object, singelton
    struct Lottery has key {
        id: UID,
        balance: Balance<SUI>,
    }

    fun init(ctx: &mut TxContext) {
        let lottery = Lottery {
            id: object::new(ctx),
            balance: zero()
        };
        sui::transfer::share_object(lottery);
    }

    // Ticket can win with probability 1%, and then receive 100 tokens.
    entry public fun buy_ticket(lottery: &mut Lottery, coin: Coin<SUI>, ctx: &mut TxContext) {
        assert!(coin::value(&coin) == 1, 0);
        balance::join(&mut lottery.balance, coin::into_balance(coin));
        let r = randomness_keyed::new(LOTTERY_LOCK {}, ctx);
        randomness_keyed::transfer(r, tx_context::sender(ctx));
    }

    entry public fun scratch(ticket: &mut Randomness<LOTTERY_LOCK>,  sig: vector<u8>) {
        randomness_keyed::set(ticket, sig);
    }

    // takes a reward, if there is enough (else, can be taken later)
    entry public fun use_ticket(lottery: &mut Lottery, ticket: Randomness<LOTTERY_LOCK>, ctx: &mut TxContext) {
        let random_bytes = option::borrow(randomness_keyed::value(&ticket));
        let first_byte = vector::borrow(random_bytes, 0);
        // TODO: make it secure...
        if (*first_byte % 100 == 0) {
            assert!(balance::value(&lottery.balance) > 99, 0);
            let coin = coin::from_balance(balance::split(&mut lottery.balance, 100), ctx);
            sui::pay::keep(coin, ctx);
        };
        randomness_keyed::destroy(ticket);
    }
}

////////////////////////////////////

// example of a lottery (1 out of n) with randomness as a shared object
module sui::lottery_example {
    use std::vector;
    use sui::balance::{Self, Balance, zero};
    use sui::coin::{Self, Coin};
    use sui::object::{Self, UID, ID, id};
    use sui::randomness_keyed::{Self, Randomness};
    use sui::sui::SUI;
    use sui::tx_context::{Self, TxContext};
    use std::option;
    use std::option::Option;

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
        assert!(option::is_none(&lottery.randomness_id), 1);
        assert!(lottery.participants < 100, 0); // just to simplify the modulo below
        assert!(coin::value(&coin) == 1, 0);
        balance::join(&mut lottery.balance, coin::into_balance(coin));
        let r = randomness_keyed::new(LOTTERY_LOCK{}, ctx);
        let ticket = Ticket {
            id: object::new(ctx),
            lottery_id: id(lottery),
            participant_id: lottery.participants,
        };
        lottery.participants = lottery.participants + 1;
        sui::transfer::transfer(ticket, tx_context::sender(ctx));
        randomness_keyed::transfer(r, tx_context::sender(ctx));
    }

    // Stop selling tickets and create a randomness that will determine the winner.
    entry fun close(lottery: &mut Lottery, ctx: &mut TxContext) {
        let r = randomness_keyed::new(LOTTERY_LOCK {}, ctx);
        let randomness_id = id(&r);
        randomness_keyed::share_object(r);
        lottery.randomness_id = option::some(randomness_id);
    }

    entry fun set_randomness(lottery: &Lottery, randomness: &mut Randomness<LOTTERY_LOCK>, sig: vector<u8>) {
        assert!(lottery.randomness_id == option::some(id(randomness)), 1);
        randomness_keyed::set(randomness, sig);
    }

    entry fun use_ticket(lottery: &mut Lottery, randomness: &Randomness<LOTTERY_LOCK>, ticket: Ticket, ctx: &mut TxContext) {
        assert!(option::is_some(randomness_keyed::value(randomness)), 11);
        assert!(*option::borrow(&lottery.randomness_id) == id(randomness), 13);
        let random_bytes = option::borrow(randomness_keyed::value(randomness));
        let first_byte = vector::borrow(random_bytes, 0);
        // TODO make it secure
        if (*first_byte % lottery.participants == ticket.participant_id) {
            let amount = balance::value(&lottery.balance);
            let coin = coin::from_balance(balance::split(&mut lottery.balance, amount), ctx);
            sui::pay::keep(coin, ctx);
        };
        let Ticket { id, lottery_id:_, participant_id:_  } = ticket;
        object::delete(id);
    }
}


// Example of a game NFTs

module sui::game_nfts_example {
    use std::vector;
    use sui::coin::{Self, Coin};
    use sui::object::{Self, UID};
    use sui::randomness_keyed::{Self, Randomness};
    use sui::sui::SUI;
    use sui::tx_context::{Self, TxContext};
    use std::option;
    use sui::transfer;

    // Make sure only the current module can access Randomness it creates.
    struct GAME_LOCK has drop {}

    struct GameElement has key {
        id: UID,
        type: u8,
    }

    const creator: address = 0x1;

    entry public fun buy_random_element(coin: Coin<SUI>, ctx: &mut TxContext) {
        assert!(coin::value(&coin) == 1, 0);
        transfer::transfer(coin, creator);
        let r = randomness_keyed::new(GAME_LOCK {}, ctx);
        randomness_keyed::transfer(r, tx_context::sender(ctx));
    }

    entry public fun get_element(ticket: Randomness<GAME_LOCK>,  sig: vector<u8>, ctx: &mut TxContext) {
        randomness_keyed::set(&mut ticket, sig);
        let random_bytes = option::borrow(randomness_keyed::value(&ticket));
        let first_byte = vector::borrow(random_bytes, 0);
        let e = GameElement { id: object::new(ctx), type: *first_byte };
        transfer::transfer(e, tx_context::sender(ctx));
        randomness_keyed::destroy(ticket);
    }
}
