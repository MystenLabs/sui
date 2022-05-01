// Copyright (c) 2022, Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

/// Module representing a common type for regulated coins. Features balance
/// accessors which can be used to implement a RegulatedCoin interface.
///
/// To implement any of the methods, module defining the type for the currency
/// is expected to implement the main set of methods such as `borrow()`,
/// `borrow_mut()` and `zero()`.
///
/// Each of the methods of this module requires a Witness struct to be sent.
module RC::RegulatedCoin {
    use Sui::Balance::{Self, Balance};
    use Sui::TxContext::{Self, TxContext};
    use Sui::ID::VersionedID;

    /// The RegulatedCoin struct; holds a common `Balance<T>` which is compatible
    /// with all the other Coins and methods, as well as the `owner` field, which
    /// can be used for additional security/regulation implementations.
    struct RegulatedCoin<phantom T> has key {
        id: VersionedID,
        balance: Balance<T>,
        owner: address
    }

    /// Get the `RegulatedCoin.balance.value` field;
    public fun value<T>(c: &RegulatedCoin<T>): u64 {
        Balance::value(&c.balance)
    }

    /// Get the `RegulatedCoin.owner` field;
    public fun owner<T>(c: &RegulatedCoin<T>): address {
        c.owner
    }

    // === Necessary set of Methods (provide security guarantees and balance access) ===

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
    public fun from_balance<T: drop>(
        _: T, balance: Balance<T>, owner: address, ctx: &mut TxContext
    ): RegulatedCoin<T> {
        RegulatedCoin { id: TxContext::new_id(ctx), balance, owner }
    }

    /// Destroy `RegulatedCoin` and return its `Balance`;
    public fun into_balance<T: drop>(_: T, coin: RegulatedCoin<T>): Balance<T> {
        let RegulatedCoin { balance, owner: _, id } = coin;
        Sui::ID::delete(id);
        balance
    }

    // === Optional Methods (can be used for simpler implementation of basic operations) ===

    /// Join Balances of a `RegulatedCoin` c1 and `RegulatedCoin` c2.
    public fun join<T: drop>(_: T, c1: &mut RegulatedCoin<T>, c2: RegulatedCoin<T>) {
        let RegulatedCoin { id, balance, owner: _ } = c2;
        Balance::join(&mut c1.balance, balance);
        Sui::ID::delete(id);
    }

    /// Subtract `RegulatedCoin` with `value` from `RegulatedCoin`.
    ///
    /// This method does not provide any checks by default and can possibly lead to mocking
    /// behavior of `RegulatedCoin::zero()` when a value is 0. So in case empty balances
    /// should not be allowed, this method should be additionally protected against zero value.
    public fun split<T: drop>(
        witness: T, c1: &mut RegulatedCoin<T>, owner: address, value: u64, ctx: &mut TxContext
    ): RegulatedCoin<T> {
        let balance = Balance::split(&mut c1.balance, value);
        from_balance(witness, balance, owner, ctx)
    }
}

/// ABC is a RegulatedCoin which:
///
/// - is managed account creation (only admins can create a new balance)
/// - has a denylist for addresses managed by the coin admins
/// - has restricted transfers which can not be taken by anyone except the recipient
module ABC::ABC {
    use RC::RegulatedCoin::{Self as RC, RegulatedCoin as RC};
    use Sui::TxContext::{Self, TxContext};
    use Sui::Balance::{Self, Balance};
    use Sui::Coin::{Self, TreasuryCap};
    use Sui::ID::{Self, VersionedID};
    use Sui::Transfer;

    /// The ticker of ABC regulated token
    struct ABC has drop {}

    /// A restricted transfer of ABC to another account
    struct Transfer has key {
        id: VersionedID,
        balance: Balance<ABC>,
        to: address
    }

    /// For when an attempting to interact with another account's RegulatedCoin<ABC>.
    const ENotOwner: u64 = 1;

    /// Create the ABC currency and send the TreasuryCap to the creator
    /// as well as the first (and empty) balance of the RegulatedCoin<ABC>.
    fun init(ctx: &mut TxContext) {
        let treasury_cap = Coin::create_currency(ABC {}, ctx);
        let sender = TxContext::sender(ctx);

        Transfer::transfer(zero(sender, ctx), sender);
        Transfer::transfer(treasury_cap, sender);
    }

    // === Minting and burning entrypoints ===

    /// Create an empty `RC<ABC>` instance for account `for`. TreasuryCap is passed for
    /// authentification purposes - only admin can create new accounts.
    public(script) fun create(_: &TreasuryCap<ABC>, for: address, ctx: &mut TxContext) {
        Transfer::transfer(zero(for, ctx), for)
    }

    /// Mint more ABC. Requires TreasuryCap for authorization, so can only be done by admins.
    public(script) fun mint(treasury: &mut TreasuryCap<ABC>, owned: &mut RC<ABC>, value: u64, _: &mut TxContext) {
        Balance::join(borrow_mut(owned), Coin::mint_balance(value, treasury))
    }

    /// Transfer entrypoint - create a restricted `Transfer` instance and transfer it to the
    /// `to` account for being accepted later.
    /// Fails if sender is not an owner of the `RegulatedCoin`.
    public(script) fun transfer(coin: &mut RC<ABC>, value: u64, to: address, ctx: &mut TxContext) {
        assert!(TxContext::sender(ctx) == RC::owner(coin), ENotOwner);

        Transfer::transfer(Transfer {
            to,
            id: TxContext::new_id(ctx),
            balance: Balance::split(borrow_mut(coin), value),
        }, to)
    }

    /// Accept an incoming transfer by joining an incoming balance with an owned one.
    /// Fails if the `RegulatedCoin<ABC>.owner` does not match `Transfer.to`;
    public(script) fun accept_transfer(coin: &mut RC<ABC>, transfer: Transfer, _: &mut TxContext) {
        assert!(RC::owner(coin) == transfer.to, ENotOwner);

        let Transfer { id, balance, to: _ } = transfer;
        Balance::join(borrow_mut(coin), balance);
        ID::delete(id)
    }

    // === Private implementations accessors and type morphing ===

    fun borrow(coin: &RC<ABC>): &Balance<ABC> { RC::borrow(ABC {}, coin) }
    fun borrow_mut(coin: &mut RC<ABC>): &mut Balance<ABC> { RC::borrow_mut(ABC {}, coin) }
    fun zero(owner: address, ctx: &mut TxContext): RC<ABC> { RC::zero(ABC {}, owner, ctx) }

    fun into_balance(coin: RC<ABC>): Balance<ABC> { RC::into_balance(ABC {}, coin) }
    fun from_balance(balance: Balance<ABC>, owner: address, ctx: &mut TxContext): RC<ABC> {
        RC::from_balance(ABC {}, balance, owner, ctx)
    }

    // === Testing utilities ===

    #[test_only] public fun init_for_testing(ctx: &mut TxContext) { init(ctx) }
    #[test_only] public fun borrow_for_testing(coin: &RC<ABC>): &Balance<ABC> { borrow(coin) }
    #[test_only] public fun borrow_mut_for_testing(coin: &mut RC<ABC>): &Balance<ABC> { borrow_mut(coin) }
}

#[test_only]
module ABC::Tests {
    use ABC::ABC::{Self, ABC};
    use RC::RegulatedCoin::{Self as RC, RegulatedCoin as RC};

    use Sui::Coin::TreasuryCap;
    use Sui::TestScenario::{Self, Scenario, next_tx, ctx};

    // === Test handlers; this trick helps reusing scenarios ==

    #[test] public(script) fun test_minting_() { test_minting(&mut scenario()) }
    #[test] public(script) fun test_creation_() { test_creation(&mut scenario()) }
    #[test] public(script) fun test_transfer_() { test_transfer(&mut scenario()) }

    // === Helpers and basic test organization ===

    fun scenario(): Scenario { TestScenario::begin(&@ABC) }
    fun people(): (address, address, address) { (@0xABC, @0xE05, @0xFACE) }

    // Admin creates a regulated coin ABC and mints 1,000,000 of it.
    public(script) fun test_minting(test: &mut Scenario) {
        let (admin, _, _) = people();

        next_tx(test, &admin); {
            ABC::init_for_testing(ctx(test))
        };

        next_tx(test, &admin); {
            let cap = TestScenario::take_owned<TreasuryCap<ABC>>(test);
            let coin = TestScenario::take_owned<RC<ABC>>(test);

            ABC::mint(&mut cap, &mut coin, 1000000, ctx(test));

            assert!(RC::value(&coin) == 1000000, 0);

            TestScenario::return_owned(test, cap);
            TestScenario::return_owned(test, coin);
        }
    }

    // Admin creates an empty balance for the `user1`.
    public(script) fun test_creation(test: &mut Scenario) {
        let (admin, user1, _) = people();

        test_minting(test);

        next_tx(test, &admin); {
            let cap = TestScenario::take_owned<TreasuryCap<ABC>>(test);

            ABC::create(&cap, user1, ctx(test));

            TestScenario::return_owned(test, cap);
        };

        next_tx(test, &user1); {
            let coin = TestScenario::take_owned<RC<ABC>>(test);

            assert!(RC::owner(&coin) == user1, 1);
            assert!(RC::value(&coin) == 0, 2);

            TestScenario::return_owned(test, coin);
        };
    }

    // Admin transfers 500,000 coins to `user1`.
    // User1 accepts the transfer and checks his balance.
    public(script) fun test_transfer(test: &mut Scenario) {
        let (admin, user1, _) = people();

        test_creation(test);

        next_tx(test, &admin); {
            let coin = TestScenario::take_owned<RC<ABC>>(test);

            ABC::transfer(&mut coin, 500000, user1, ctx(test));

            TestScenario::return_owned(test, coin);
        };

        next_tx(test, &user1); {
            let coin = TestScenario::take_owned<RC<ABC>>(test);
            let transfer = TestScenario::take_owned<ABC::Transfer>(test);

            ABC::accept_transfer(&mut coin, transfer, ctx(test));

            assert!(RC::value(&coin) == 500000, 3);

            TestScenario::return_owned(test, coin);
        };
    }
}































module RC::FREE {
    use Sui::Balance::Balance;
    use Sui::TxContext::{Self, TxContext};
    use RC::RegulatedCoin::{Self as C, RegulatedCoin};

    struct FREE has drop {}

    // === implement the interface for the RegulatedCoin ===

    public fun borrow(coin: &RegulatedCoin<FREE>): &Balance<FREE> { C::borrow(FREE {}, coin) }
    public fun borrow_mut(coin: &mut RegulatedCoin<FREE>): &mut Balance<FREE> { C::borrow_mut(FREE {}, coin) }
    public fun from_balance(balance: Balance<FREE>, ctx: &mut TxContext): RegulatedCoin<FREE> { C::from_balance(FREE {}, balance, TxContext::sender(ctx), ctx) }
    public fun into_balance(coin: RegulatedCoin<FREE>): Balance<FREE> { C::into_balance(FREE {}, coin) }

    // === and that's it (+ minting and currency creation) ===
}

// A very RESTricted coin.
module RC::REST {
    use RC::RegulatedCoin::{Self as C, RegulatedCoin};

    use Sui::Balance::{Self, Balance};
    use Sui::Coin::{Self, TreasuryCap};
    use Sui::TxContext::{Self, TxContext};
    use Sui::Transfer;

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
}

module RC::RestrictedStake {
    use RC::REST::{Self, REST};
    use RC::RegulatedCoin::RegulatedCoin;

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
