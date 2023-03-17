// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

/// Module representing a common type for regulated coins. Features balance
/// accessors which can be used to implement a RegulatedCoin interface.
///
/// To implement any of the methods, module defining the type for the currency
/// is expected to implement the main set of methods such as `borrow()`,
/// `borrow_mut()` and `zero()`.
///
/// Each of the methods of this module requires a Witness struct to be sent.
module rc::regulated_coin {
    use sui::balance::{Self, Balance};
    use sui::tx_context::TxContext;
    use sui::object::{Self, UID};

    /// The RegulatedCoin struct; holds a common `Balance<T>` which is compatible
    /// with all the other Coins and methods, as well as the `creator` field, which
    /// can be used for additional security/regulation implementations.
    struct RegulatedCoin<phantom T> has key, store {
        id: UID,
        balance: Balance<T>,
        creator: address
    }

    /// Get the `RegulatedCoin.balance.value` field;
    public fun value<T>(c: &RegulatedCoin<T>): u64 {
        balance::value(&c.balance)
    }

    /// Get the `RegulatedCoin.creator` field;
    public fun creator<T>(c: &RegulatedCoin<T>): address {
        c.creator
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
    public fun zero<T: drop>(_: T, creator: address, ctx: &mut TxContext): RegulatedCoin<T> {
        RegulatedCoin { id: object::new(ctx), balance: balance::zero(), creator }
    }

    /// Build a transferable `RegulatedCoin` from a `Balance`;
    public fun from_balance<T: drop>(
        _: T, balance: Balance<T>, creator: address, ctx: &mut TxContext
    ): RegulatedCoin<T> {
        RegulatedCoin { id: object::new(ctx), balance, creator }
    }

    /// Destroy `RegulatedCoin` and return its `Balance`;
    public fun into_balance<T: drop>(_: T, coin: RegulatedCoin<T>): Balance<T> {
        let RegulatedCoin { balance, creator: _, id } = coin;
        sui::object::delete(id);
        balance
    }

    // === Optional Methods (can be used for simpler implementation of basic operations) ===

    /// Join Balances of a `RegulatedCoin` c1 and `RegulatedCoin` c2.
    public fun join<T: drop>(witness: T, c1: &mut RegulatedCoin<T>, c2: RegulatedCoin<T>) {
        balance::join(&mut c1.balance, into_balance(witness, c2));
    }

    /// Subtract `RegulatedCoin` with `value` from `RegulatedCoin`.
    ///
    /// This method does not provide any checks by default and can possibly lead to mocking
    /// behavior of `Regulatedcoin::zero()` when a value is 0. So in case empty balances
    /// should not be allowed, this method should be additionally protected against zero value.
    public fun split<T: drop>(
        witness: T, c1: &mut RegulatedCoin<T>, creator: address, value: u64, ctx: &mut TxContext
    ): RegulatedCoin<T> {
        let balance = balance::split(&mut c1.balance, value);
        from_balance(witness, balance, creator, ctx)
    }
}

/// Abc is a RegulatedCoin which:
///
/// - is managed account creation (only admins can create a new balance)
/// - has a denylist for addresses managed by the coin admins
/// - has restricted transfers which can not be taken by anyone except the recipient
module abc::abc {
    use rc::regulated_coin::{Self as rcoin, RegulatedCoin as RCoin};
    use sui::tx_context::{Self, TxContext};
    use sui::balance::{Self, Supply, Balance};
    use sui::object::{Self, UID};
    use sui::coin::{Self, Coin};
    use sui::transfer;
    use std::vector;

    /// The ticker of Abc regulated token
    struct Abc has drop {}

    /// A restricted transfer of Abc to another account.
    struct Transfer has key {
        id: UID,
        balance: Balance<Abc>,
        to: address,
    }

    /// A registry of addresses banned from using the coin.
    struct Registry has key {
        id: UID,
        banned: vector<address>,
        swapped_amount: u64,
    }

    /// A AbcTreasuryCap for the balance::Supply.
    struct AbcTreasuryCap has key, store {
        id: UID,
        supply: Supply<Abc>
    }

    /// For when an attempting to interact with another account's RegulatedCoin<Abc>.
    const ENotOwner: u64 = 1;

    /// For when address has been banned and someone is trying to access the balance
    const EAddressBanned: u64 = 2;

    /// Create the Abc currency and send the AbcTreasuryCap to the creator
    /// as well as the first (and empty) balance of the RegulatedCoin<Abc>.
    ///
    /// Also creates a shared Registry which holds banned addresses.
    fun init(ctx: &mut TxContext) {
        let sender = tx_context::sender(ctx);
        let treasury_cap = AbcTreasuryCap {
            id: object::new(ctx),
            supply: balance::create_supply(Abc {})
        };

        transfer::public_transfer(zero(sender, ctx), sender);
        transfer::public_transfer(treasury_cap, sender);

        transfer::share_object(Registry {
            id: object::new(ctx),
            banned: vector::empty(),
            swapped_amount: 0,
        });
    }

    // === Getters section: Registry ===

    /// Get total amount of `Coin` from the `Registry`.
    public fun swapped_amount(r: &Registry): u64 {
        r.swapped_amount
    }

    /// Get vector of banned addresses from `Registry`.
    public fun banned(r: &Registry): &vector<address> {
        &r.banned
    }

    // === Admin actions: creating balances, minting coins and banning addresses ===

    /// Create an empty `RCoin<Abc>` instance for account `for`. AbcTreasuryCap is passed for
    /// authentication purposes - only admin can create new accounts.
    public entry fun create(_: &AbcTreasuryCap, for: address, ctx: &mut TxContext) {
        transfer::public_transfer(zero(for, ctx), for)
    }

    /// Mint more Abc. Requires AbcTreasuryCap for authorization, so can only be done by admins.
    public entry fun mint(treasury: &mut AbcTreasuryCap, owned: &mut RCoin<Abc>, value: u64) {
        balance::join(borrow_mut(owned), balance::increase_supply(&mut treasury.supply, value));
    }

    /// Burn `value` amount of `RCoin<Abc>`. Requires AbcTreasuryCap for authorization, so can only be done by admins.
    ///
    /// TODO: Make AbcTreasuryCap a part of Balance module instead of Coin.
    public entry fun burn(treasury: &mut AbcTreasuryCap, owned: &mut RCoin<Abc>, value: u64) {
        balance::decrease_supply(
            &mut treasury.supply,
            balance::split(borrow_mut(owned), value)
        );
    }

    /// Ban some address and forbid making any transactions from or to this address.
    /// Only owner of the AbcTreasuryCap can perform this action.
    public entry fun ban(_cap: &AbcTreasuryCap, registry: &mut Registry, to_ban: address) {
        vector::push_back(&mut registry.banned, to_ban)
    }

    // === Public: Regulated transfers ===

    /// Transfer entrypoint - create a restricted `Transfer` instance and transfer it to the
    /// `to` account for being accepted later.
    /// Fails if sender is not an creator of the `RegulatedCoin` or if any of the parties is in
    /// the ban list in Registry.
    public entry fun transfer(r: &Registry, coin: &mut RCoin<Abc>, value: u64, to: address, ctx: &mut TxContext) {
        let sender = tx_context::sender(ctx);

        assert!(rcoin::creator(coin) == sender, ENotOwner);
        assert!(vector::contains(&r.banned, &to) == false, EAddressBanned);
        assert!(vector::contains(&r.banned, &sender) == false, EAddressBanned);

        transfer::transfer(Transfer {
            to,
            id: object::new(ctx),
            balance: balance::split(borrow_mut(coin), value),
        }, to)
    }

    /// Accept an incoming transfer by joining an incoming balance with an owned one.
    ///
    /// Fails if:
    /// 1. the `RegulatedCoin<Abc>.creator` does not match `Transfer.to`;
    /// 2. the address of the creator/recipient is banned;
    public entry fun accept_transfer(r: &Registry, coin: &mut RCoin<Abc>, transfer: Transfer) {
        let Transfer { id, balance, to } = transfer;

        assert!(rcoin::creator(coin) == to, ENotOwner);
        assert!(vector::contains(&r.banned, &to) == false, EAddressBanned);

        balance::join(borrow_mut(coin), balance);
        object::delete(id)
    }

    // === Public: Swap RegulatedCoin <-> Coin ===

    /// Take `value` amount of `RegulatedCoin` and make it freely transferable by wrapping it into
    /// a `Coin`. Update `Registry` to keep track of the swapped amount.
    ///
    /// Fails if:
    /// 1. `RegulatedCoin<Abc>.creator` was banned;
    /// 2. `RegulatedCoin<Abc>` is not owned by the tx sender;
    public entry fun take(r: &mut Registry, coin: &mut RCoin<Abc>, value: u64, ctx: &mut TxContext) {
        let sender = tx_context::sender(ctx);

        assert!(rcoin::creator(coin) == sender, ENotOwner);
        assert!(vector::contains(&r.banned, &sender) == false, EAddressBanned);

        // Update swapped amount for Registry to keep track of non-regulated amounts.
        r.swapped_amount = r.swapped_amount + value;

        transfer::public_transfer(coin::take(borrow_mut(coin), value, ctx), sender);
    }

    /// Take `Coin` and put to the `RegulatedCoin`'s balance.
    ///
    /// Fails if:
    /// 1. `RegulatedCoin<Abc>.creator` was banned;
    /// 2. `RegulatedCoin<Abc>` is not owned by the tx sender;
    public entry fun put_back(r: &mut Registry, rc_coin: &mut RCoin<Abc>, coin: Coin<Abc>, ctx: &TxContext) {
        let balance = coin::into_balance(coin);
        let sender = tx_context::sender(ctx);

        assert!(rcoin::creator(rc_coin) == sender, ENotOwner);
        assert!(vector::contains(&r.banned, &sender) == false, EAddressBanned);

        // Update swapped amount as in `swap_regulated`.
        r.swapped_amount = r.swapped_amount - balance::value(&balance);

        balance::join(borrow_mut(rc_coin), balance);
    }

    // === Private implementations accessors and type morphing ===

    fun borrow(coin: &RCoin<Abc>): &Balance<Abc> { rcoin::borrow(Abc {}, coin) }
    fun borrow_mut(coin: &mut RCoin<Abc>): &mut Balance<Abc> { rcoin::borrow_mut(Abc {}, coin) }
    fun zero(creator: address, ctx: &mut TxContext): RCoin<Abc> { rcoin::zero(Abc {}, creator, ctx) }

    fun into_balance(coin: RCoin<Abc>): Balance<Abc> { rcoin::into_balance(Abc {}, coin) }
    fun from_balance(balance: Balance<Abc>, creator: address, ctx: &mut TxContext): RCoin<Abc> {
        rcoin::from_balance(Abc {}, balance, creator, ctx)
    }

    // === Testing utilities ===

    #[test_only] public fun init_for_testing(ctx: &mut TxContext) { init(ctx) }
    #[test_only] public fun borrow_for_testing(coin: &RCoin<Abc>): &Balance<Abc> { borrow(coin) }
    #[test_only] public fun borrow_mut_for_testing(coin: &mut RCoin<Abc>): &Balance<Abc> { borrow_mut(coin) }
}

#[test_only]
/// Tests for the abc module. They are sequential and based on top of each other.
/// ```
/// * - test_minting
/// |   +-- test_creation
/// |       +-- test_transfer
/// |           +-- test_burn
/// |           +-- test_take
/// |               +-- test_put_back
/// |           +-- test_ban
/// |               +-- test_address_banned_fail
/// |               +-- test_different_account_fail
/// |               +-- test_not_owned_balance_fail
/// ```
module abc::tests {
    use abc::abc::{Self, Abc, AbcTreasuryCap, Registry};
    use rc::regulated_coin::{Self as rcoin, RegulatedCoin as RCoin};

    use sui::coin::{Coin};
    use sui::test_scenario::{Self, Scenario, next_tx, ctx};

    // === Test handlers; this trick helps reusing scenarios ==

    fun test_minting() {
        let scenario = scenario();
        test_minting_(&mut scenario);
        test_scenario::end(scenario);
    }
    fun test_creation() {
        let scenario = scenario();
        test_creation_(&mut scenario);
        test_scenario::end(scenario);
    }
    fun test_transfer() {
        let scenario = scenario();
        test_transfer_(&mut scenario);
        test_scenario::end(scenario);
    }
    fun test_burn() {
        let scenario = scenario();
        test_burn_(&mut scenario);
        test_scenario::end(scenario);
    }
    fun test_take() {
        let scenario = scenario();
        test_take_(&mut scenario);
        test_scenario::end(scenario);
    }
    fun test_put_back() {
        let scenario = scenario();
        test_put_back_(&mut scenario);
        test_scenario::end(scenario);
    }
    fun test_ban() {
        let scenario = scenario();
        test_ban_(&mut scenario);
        test_scenario::end(scenario);
    }

    #[test]
    #[expected_failure(abort_code = abc::abc::EAddressBanned)]
    fun test_address_banned_fail() {
        let scenario = scenario();
        test_address_banned_fail_(&mut scenario);
        test_scenario::end(scenario);
    }

    #[test]
    #[expected_failure(abort_code = abc::abc::EAddressBanned)]
    fun test_different_account_fail() {
        let scenario = scenario();
        test_different_account_fail_(&mut scenario);
        test_scenario::end(scenario);
    }

    #[test]
    #[expected_failure(abort_code = abc::abc::ENotOwner)]
    fun test_not_owned_balance_fail() {
        let scenario = scenario();
        test_not_owned_balance_fail_(&mut scenario);
        test_scenario::end(scenario);
    }

    // === Helpers and basic test organization ===

    fun scenario(): Scenario { test_scenario::begin(@0xAbc) }
    fun people(): (address, address, address) { (@0xAbc, @0xE05, @0xFACE) }

    // Admin creates a regulated coin Abc and mints 1,000,000 of it.
    fun test_minting_(test: &mut Scenario) {
        let (admin, _, _) = people();

        next_tx(test, admin);
        {
            abc::init_for_testing(ctx(test))
        };

        next_tx(test, admin);
        {
            let cap = test_scenario::take_from_sender<AbcTreasuryCap>(test);
            let coin = test_scenario::take_from_sender<RCoin<Abc>>(test);

            abc::mint(&mut cap, &mut coin, 1000000);

            assert!(rcoin::value(&coin) == 1000000, 0);

            test_scenario::return_to_sender(test, cap);
            test_scenario::return_to_sender(test, coin);
        }
    }

    // Admin creates an empty balance for the `user1`.
    fun test_creation_(test: &mut Scenario) {
        let (admin, user1, _) = people();

        test_minting_(test);

        next_tx(test, admin);
        {
            let cap = test_scenario::take_from_sender<AbcTreasuryCap>(test);

            abc::create(&cap, user1, ctx(test));

            test_scenario::return_to_sender(test, cap);
        };

        next_tx(test, user1);
        {
            let coin = test_scenario::take_from_sender<RCoin<Abc>>(test);

            assert!(rcoin::creator(&coin) == user1, 1);
            assert!(rcoin::value(&coin) == 0, 2);

            test_scenario::return_to_sender(test, coin);
        };
    }

    // Admin transfers 500,000 coins to `user1`.
    // User1 accepts the transfer and checks his balance.
    fun test_transfer_(test: &mut Scenario) {
        let (admin, user1, _) = people();

        test_creation_(test);

        next_tx(test, admin);
        {
            let coin = test_scenario::take_from_sender<RCoin<Abc>>(test);
            let reg = test_scenario::take_shared<Registry>(test);
            let reg_ref = &mut reg;

            abc::transfer(reg_ref, &mut coin, 500000, user1, ctx(test));

            test_scenario::return_shared(reg);
            test_scenario::return_to_sender(test, coin);
        };

        next_tx(test, user1);
        {
            let coin = test_scenario::take_from_sender<RCoin<Abc>>(test);
            let transfer = test_scenario::take_from_sender<abc::Transfer>(test);
            let reg = test_scenario::take_shared<Registry>(test);
            let reg_ref = &mut reg;

            abc::accept_transfer(reg_ref, &mut coin, transfer);

            assert!(rcoin::value(&coin) == 500000, 3);

            test_scenario::return_shared(reg);
            test_scenario::return_to_sender(test, coin);
        };
    }

    // Admin burns 100,000 of `RCoin<Abc>`
    fun test_burn_(test: &mut Scenario) {
        let (admin, _, _) = people();

        test_transfer_(test);

        next_tx(test, admin);
        {
            let coin = test_scenario::take_from_sender<RCoin<Abc>>(test);
            let treasury_cap = test_scenario::take_from_sender<AbcTreasuryCap>(test);

            abc::burn(&mut treasury_cap, &mut coin, 100000);

            assert!(rcoin::value(&coin) == 400000, 4);

            test_scenario::return_to_sender(test, treasury_cap);
            test_scenario::return_to_sender(test, coin);
        };
    }

    // User1 cashes 100,000 of his `RegulatedCoin` into a `Coin`;
    // User1 sends Coin<Abc> it to `user2`.
    fun test_take_(test: &mut Scenario) {
        let (_, user1, user2) = people();

        test_transfer_(test);

        next_tx(test, user1);
        {
            let coin = test_scenario::take_from_sender<RCoin<Abc>>(test);
            let reg = test_scenario::take_shared<Registry>(test);
            let reg_ref = &mut reg;

            abc::take(reg_ref, &mut coin, 100000, ctx(test));

            assert!(abc::swapped_amount(reg_ref) == 100000, 5);
            assert!(rcoin::value(&coin) == 400000, 5);

            test_scenario::return_shared( reg);
            test_scenario::return_to_sender(test, coin);
        };

        next_tx(test, user1);
        {
            let coin = test_scenario::take_from_sender<Coin<Abc>>(test);
            sui::transfer::public_transfer(coin, user2);
        };
    }

    // User2 sends his `Coin<Abc>` to `admin`.
    // Admin puts this coin to his RegulatedCoin balance.
    fun test_put_back_(test: &mut Scenario) {
        let (admin, _, user2) = people();

        test_take_(test);

        next_tx(test, user2);
        {
            let coin = test_scenario::take_from_sender<Coin<Abc>>(test);
            sui::transfer::public_transfer(coin, admin);
        };

        next_tx(test, admin);
        {
            let coin = test_scenario::take_from_sender<Coin<Abc>>(test);
            let reg_coin = test_scenario::take_from_sender<RCoin<Abc>>(test);
            let reg = test_scenario::take_shared<Registry>(test);
            let reg_ref = &mut reg;

            abc::put_back(reg_ref, &mut reg_coin, coin, ctx(test));

            test_scenario::return_to_sender(test, reg_coin);
            test_scenario::return_shared(reg);
        }
    }

    // Admin bans user1 by adding his address to the registry.
    fun test_ban_(test: &mut Scenario) {
        let (admin, user1, _) = people();

        test_transfer_(test);

        next_tx(test, admin);
        {
            let cap = test_scenario::take_from_sender<AbcTreasuryCap>(test);
            let reg = test_scenario::take_shared<Registry>(test);
            let reg_ref = &mut reg;

            abc::ban(&cap, reg_ref, user1);

            test_scenario::return_shared(reg);
            test_scenario::return_to_sender(test, cap);
        };
    }

    // Banned User1 fails to create a Transfer.
    fun test_address_banned_fail_(test: &mut Scenario) {
        let (_, user1, user2) = people();

        test_ban_(test);

        next_tx(test, user1);
        {
            let coin = test_scenario::take_from_sender<RCoin<Abc>>(test);
            let reg = test_scenario::take_shared<Registry>(test);
            let reg_ref = &mut reg;

            abc::transfer(reg_ref, &mut coin, 250000, user2, ctx(test));

            test_scenario::return_shared(reg);
            test_scenario::return_to_sender(test, coin);
        };
    }

    // User1 is banned. Admin tries to make a Transfer to User1 and fails - user banned.
    fun test_different_account_fail_(test: &mut Scenario) {
        let (admin, user1, _) = people();

        test_ban_(test);

        next_tx(test, admin);
        {
            let coin = test_scenario::take_from_sender<RCoin<Abc>>(test);
            let reg = test_scenario::take_shared<Registry>(test);
            let reg_ref = &mut reg;

            abc::transfer(reg_ref, &mut coin, 250000, user1, ctx(test));

            test_scenario::return_shared(reg);
            test_scenario::return_to_sender(test, coin);
        };
    }

    // User1 is banned and transfers the whole balance to User2.
    // User2 tries to use this balance and fails.
    fun test_not_owned_balance_fail_(test: &mut Scenario) {
        let (_, user1, user2) = people();

        test_ban_(test);

        next_tx(test, user1);
        {
            let coin = test_scenario::take_from_sender<RCoin<Abc>>(test);
            sui::transfer::public_transfer(coin, user2);
        };

        next_tx(test, user2);
        {
            let coin = test_scenario::take_from_sender<RCoin<Abc>>(test);
            let reg = test_scenario::take_shared<Registry>(test);
            let reg_ref = &mut reg;

            abc::transfer(reg_ref, &mut coin, 500000, user1, ctx(test));

            test_scenario::return_shared(reg);
            test_scenario::return_to_sender(test, coin);
        }
    }
}
