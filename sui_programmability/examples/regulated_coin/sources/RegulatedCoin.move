// Copyright (c) 2022, Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

module Stable::RegulatedCoin {
    use Sui::Balance::{Self, Balance};
    use Sui::TxContext::{Self, TxContext};
    use Sui::ID::VersionedID;

    struct RegulatedCoin<phantom T> has key {
        id: VersionedID,
        balance: Balance<T>,
        owner: address
    }

    /// Get the `RegulatedCoin.owner` field;
    public fun owner<T>(c: &RegulatedCoin<T>): address {
        c.owner
    }

    /// Get an immutable reference to the Balance of a RegulatedCoin;
    public fun borrow<T: drop>(_: T, coin: &RegulatedCoin<T>): &Balance<T> {
        &coin.balance
    }

    /// Get a mutable reference to the Balance of a RegulatedCoin;
    public fun borrow_mut<T: drop>(_: T, coin: &mut RegulatedCoin<T>): &mut Balance<T> {
        &mut coin.balance
    }

    /// Author of the currency can restrict who is allowed to create new balances;
    public fun zero<T: drop>(_: T, owner: address, ctx: &mut TxContext): RegulatedCoin<T> {
        RegulatedCoin { id: TxContext::new_id(ctx), balance: Balance::zero(), owner }
    }

    /// Build a transferable `RegulatedCoin` from a `Balance`;
    public fun from_balance<T: drop>(_: T, balance: Balance<T>, owner: address, ctx: &mut TxContext): RegulatedCoin<T> {
        RegulatedCoin { id: TxContext::new_id(ctx), balance, owner }
    }

    /// Turn `RegulatedCoin` into a `Balance`;
    public fun into_balance<T: drop>(_: T, coin: RegulatedCoin<T>): Balance<T> {
        let RegulatedCoin { balance, owner: _, id } = coin;
        Sui::ID::delete(id);
        balance
    }

    public fun join<T: drop>(_: T, c1: &mut RegulatedCoin<T>, c2: RegulatedCoin<T>) {
        let RegulatedCoin { id, balance, owner: _ } = c2;
        Balance::join(&mut c1.balance, balance);
        Sui::ID::delete(id);
    }

    public fun split<T: drop>(witness: T, c1: &mut RegulatedCoin<T>, owner: address, value: u64, ctx: &mut TxContext): RegulatedCoin<T> {
        let balance = Balance::split(&mut c1.balance, value);
        from_balance(witness, balance, owner, ctx)
    }
}

module Stable::FREE {
    use Sui::Balance::Balance;
    use Sui::TxContext::{Self, TxContext};
    use Stable::RegulatedCoin::{Self as C, RegulatedCoin};

    struct FREE has drop {}

    // === implement the interface for the RegulatedCoin ===

    public fun borrow(coin: &RegulatedCoin<FREE>): &Balance<FREE> { C::borrow(FREE {}, coin) }
    public fun borrow_mut(coin: &mut RegulatedCoin<FREE>): &mut Balance<FREE> { C::borrow_mut(FREE {}, coin) }
    public fun from_balance(balance: Balance<FREE>, ctx: &mut TxContext): RegulatedCoin<FREE> { C::from_balance(FREE {}, balance, TxContext::sender(ctx), ctx) }
    public fun into_balance(coin: RegulatedCoin<FREE>): Balance<FREE> { C::into_balance(FREE {}, coin) }

    // === and that's it (+ minting and currency creation) ===
}

// A very RESTricted coin.
module Stable::REST {
    use Stable::RegulatedCoin::{Self as C, RegulatedCoin};

    use Sui::Balance::{Self, Balance};
    use Sui::Coin::{Self, TreasuryCap};
    use Sui::TxContext::{Self, TxContext};
    use Sui::Transfer;

    const ENotImplemented: u64 = 0;
    const ENotAllowed: u64 = 1;
    const ENotOwner: u64 = 2;

    struct REST has drop {}

    /// A restricted transfer of the Balance
    struct CoinTransfer has key {
        id: Sui::ID::VersionedID,
        balance: Balance<REST>,
        to: address
    }

    /// Currently let's just use Coin::TreasuryCap functionality
    fun init(ctx: &mut TxContext) {
        Transfer::transfer(
            Coin::create_currency(REST {}, ctx),
            TxContext::sender(ctx)
        )
    }

    /// Only owner of the treasury cap can create new Balances; for example, after a KYC process;
    public fun create_empty_for(_cap: &TreasuryCap<REST>, for: address, ctx: &mut TxContext) {
        Transfer::transfer(C::zero(REST {}, for, ctx), for)
    }

    /// Allow borrowing as is, by default
    public fun borrow(coin: &RegulatedCoin<REST>): &Balance<REST> { C::borrow(REST {}, coin) }
    public fun borrow_mut(coin: &mut RegulatedCoin<REST>, ctx: &mut TxContext): &mut Balance<REST> {
        assert!(TxContext::sender(ctx) == C::owner(coin), ENotOwner); // only owner can access the balance
        C::borrow_mut(REST {}, coin)
    }

    // === Coin Transfers ===

    public(script) fun transfer(
        coin: &mut RegulatedCoin<REST>, value: u64, to: address, ctx: &mut TxContext
    ) {
        Transfer::transfer(CoinTransfer {
            id: TxContext::new_id(ctx),
            balance: Balance::split(borrow_mut(coin, ctx), value),
            to
        }, to)
    }

    public(script) fun accept_transfer(
        coin: &mut RegulatedCoin<REST>, transfer: CoinTransfer, ctx: &mut TxContext
    ) {
        let CoinTransfer { id, balance, to } = transfer;
        assert!(C::owner(coin) == to, ENotOwner);
        Balance::join(borrow_mut(coin, ctx), balance);
        Sui::ID::delete(id);
    }

    // === Explicit "Not Implemented" part ===

    public fun join() { abort ENotImplemented }
    public fun split() { abort ENotImplemented }
    public fun into_balance() { abort ENotImplemented }
    public fun from_balance() { abort ENotImplemented }
}

module Stable::RestrictedStake {
    use Stable::REST::{Self, REST};
    use Stable::RegulatedCoin::RegulatedCoin;

    use Sui::Coin::{Self, Coin, TreasuryCap};
    use Sui::Balance::{Self, Balance};
    use Sui::TxContext::{Self, TxContext};
    use Sui::Transfer;

    // stake token - get your money back once
    struct STAKE has drop {}

    struct StableStake has key {
        id: Sui::ID::VersionedID,
        balance: Balance<REST>,
        treasury_cap: TreasuryCap<STAKE>,
    }

    fun init(ctx: &mut TxContext) {
        Transfer::share_object(StableStake {
            id: TxContext::new_id(ctx),
            balance: Balance::zero<REST>(),
            treasury_cap: Coin::create_currency<STAKE>(STAKE{}, ctx)
        });
    }

    public(script) fun fill(
        stake: &mut StableStake,
        coin: &mut RegulatedCoin<REST>,
        value: u64,
        ctx: &mut TxContext
    ) {
        let to_fill = Balance::split(REST::borrow_mut(coin, ctx), value);
        let coin = Coin::mint<STAKE>(value, &mut stake.treasury_cap, ctx);

        Balance::join(&mut stake.balance, to_fill);
        Transfer::transfer(coin, TxContext::sender(ctx))
    }

    public(script) fun withdraw(
        stake: &mut StableStake,
        stable: &mut RegulatedCoin<REST>,
        coin: Coin<STAKE>,
        ctx: &mut TxContext
    ) {
        let balance = Balance::split(&mut stake.balance, Coin::value(&coin));

        Coin::burn(coin, &mut stake.treasury_cap);
        Balance::join(REST::borrow_mut(stable, ctx), balance);
    }
}

module Stable::Hack {
    use Stable::FREE::{Self, FREE};
    use Stable::RegulatedCoin::RegulatedCoin;
    use Sui::Balance::{Self, Balance};
    use Sui::TxContext::TxContext;
    use Sui::Coin::{Self, Coin};

    // assume it is a shared object
    struct DApp {
        balance: Balance<FREE>
    }

    // a public method
    public fun add(dapp: &mut DApp, coin: RegulatedCoin<FREE>) {
        Balance::join(&mut dapp.balance, FREE::into_balance(coin));
    }

    public fun take(dapp: &mut DApp, value: u64, ctx: &mut TxContext): RegulatedCoin<FREE> {
        let balance = Balance::split(&mut dapp.balance, value);
        FREE::from_balance(balance, ctx)
    }

    public fun take_bad(dapp: &mut DApp, value: u64, ctx: &mut TxContext): Coin<FREE> { // the problem
        let balance = Balance::split(&mut dapp.balance, value);
        Coin::from_balance(balance, ctx)
    }
}
