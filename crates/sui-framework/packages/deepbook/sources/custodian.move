// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

module deepbook::custodian {
    use sui::balance::{Self, Balance, split};
    use sui::coin::{Self, Coin};
    use sui::object::{Self, UID, ID};
    use sui::table::{Self, Table};
    use sui::tx_context::TxContext;

    friend deepbook::clob;

    // Custodian for limit orders.

    // <<<<<<<<<<<<<<<<<<<<<<<< Error codes <<<<<<<<<<<<<<<<<<<<<<<<
    const EUserBalanceDoesNotExist: u64 = 1;
    // <<<<<<<<<<<<<<<<<<<<<<<< Error codes <<<<<<<<<<<<<<<<<<<<<<<<

    struct Account<phantom T> has store {
        available_balance: Balance<T>,
        locked_balance: Balance<T>,
    }

    struct AccountCap has key, store { id: UID }

    struct Custodian<phantom T> has key, store {
        id: UID,
        /// Map from an AccountCap object ID to an Account object
        account_balances: Table<ID, Account<T>>,
    }

    /// Create an `AccountCap` that can be used across all DeepBook pool
    public fun mint_account_cap(ctx: &mut TxContext): AccountCap {
        AccountCap { id: object::new(ctx) }
    }

    public(friend) fun account_balance<Asset>(
        custodian: &Custodian<Asset>,
        user: ID
    ): (u64, u64){
        let account_balances = table::borrow(&custodian.account_balances, user);
        let avail_balance = balance::value(&account_balances.available_balance);
        let locked_balance = balance::value(&account_balances.locked_balance);
        (avail_balance, locked_balance)
    }

    public(friend) fun new<T>(ctx: &mut TxContext): Custodian<T> {
        Custodian<T> {
            id: object::new(ctx),
            account_balances: table::new(ctx),
        }
    }

    public(friend) fun withdraw_base_asset<BaseAsset>(
        custodian: &mut Custodian<BaseAsset>,
        quantity: u64,
        account_cap: &AccountCap,
        ctx: &mut TxContext
    ): Coin<BaseAsset> {
        coin::from_balance(decrease_user_available_balance<BaseAsset>(custodian, account_cap, quantity), ctx)
    }

    public(friend) fun withdraw_quote_asset<QuoteAsset>(
        custodian: &mut Custodian<QuoteAsset>,
        quantity: u64,
        account_cap: &AccountCap,
        ctx: &mut TxContext
    ): Coin<QuoteAsset> {
        coin::from_balance(decrease_user_available_balance<QuoteAsset>(custodian, account_cap, quantity), ctx)
    }

    public(friend) fun increase_user_available_balance<T>(
        custodian: &mut Custodian<T>,
        user: ID,
        quantity: Balance<T>,
    ) {
        let account = borrow_mut_account_balance<T>(custodian, user);
        balance::join(&mut account.available_balance, quantity);
    }

    public(friend) fun decrease_user_available_balance<T>(
        custodian: &mut Custodian<T>,
        account_cap: &AccountCap,
        quantity: u64,
    ): Balance<T> {
        let account = borrow_mut_account_balance<T>(custodian, object::uid_to_inner(&account_cap.id));
        balance::split(&mut account.available_balance, quantity)
    }

    public(friend) fun increase_user_locked_balance<T>(
        custodian: &mut Custodian<T>,
        account_cap: &AccountCap,
        quantity: Balance<T>,
    ) {
        let account = borrow_mut_account_balance<T>(custodian, object::uid_to_inner(&account_cap.id));
        balance::join(&mut account.locked_balance, quantity);
    }

    public(friend) fun decrease_user_locked_balance<T>(
        custodian: &mut Custodian<T>,
        user: ID,
        quantity: u64,
    ): Balance<T> {
        let account = borrow_mut_account_balance<T>(custodian, user);
        split(&mut account.locked_balance, quantity)
    }

    /// Move `quantity` from the unlocked balance of `user` to the locked balance of `user`
    public(friend) fun lock_balance<T>(
        custodian: &mut Custodian<T>,
        account_cap: &AccountCap,
        quantity: u64,
    ) {
        let to_lock = decrease_user_available_balance(custodian, account_cap, quantity);
        increase_user_locked_balance(custodian, account_cap, to_lock);
    }

    /// Move `quantity` from the locked balance of `user` to the unlocked balacne of `user`
    public(friend) fun unlock_balance<T>(
        custodian: &mut Custodian<T>,
        user: ID,
        quantity: u64,
    ) {
        let locked_balance = decrease_user_locked_balance<T>(custodian, user, quantity);
        increase_user_available_balance<T>(custodian, user, locked_balance)
    }

    public fun account_available_balance<T>(
        custodian: &Custodian<T>,
        user: ID,
    ): u64 {
        balance::value(&table::borrow(&custodian.account_balances, user).available_balance)
    }

    public fun account_locked_balance<T>(
        custodian: &Custodian<T>,
        user: ID,
    ): u64 {
        balance::value(&table::borrow(&custodian.account_balances, user).locked_balance)
    }

    fun borrow_mut_account_balance<T>(
        custodian: &mut Custodian<T>,
        user: ID,
    ): &mut Account<T> {
        if (!table::contains(&custodian.account_balances, user)) {
            table::add(
                &mut custodian.account_balances,
                user,
                Account { available_balance: balance::zero(), locked_balance: balance::zero() }
            );
        };
        table::borrow_mut(&mut custodian.account_balances, user)
    }

    fun borrow_account_balance<T>(
        custodian: &Custodian<T>,
        user: ID,
    ): &Account<T> {
        assert!(
            table::contains(&custodian.account_balances, user),
            EUserBalanceDoesNotExist
        );
        table::borrow(&custodian.account_balances, user)
    }

    #[test_only]
    friend deepbook::clob_test;
    #[test_only]
    use sui::test_scenario::{Self, Scenario};
    #[test_only]
    use sui::transfer;
    #[test_only]
    const ENull: u64 = 0;

    #[test_only]
    struct USD {}

    #[test_only]
    public fun assert_user_balance<T>(
        custodian: &Custodian<T>,
        user: ID,
        available_balance: u64,
        locked_balance: u64,
    ) {
        let user_balance = borrow_account_balance<T>(custodian, user);
        assert!(balance::value(&user_balance.available_balance) == available_balance, ENull);
        assert!(balance::value(&user_balance.locked_balance) == locked_balance, ENull)
    }

    #[test_only]
    fun setup_test(
        scenario: &mut Scenario,
    ) {
        transfer::share_object<Custodian<USD>>(new<USD>(test_scenario::ctx(scenario)));
    }

    #[test_only]
    public fun test_increase_user_available_balance<T>(
        custodian: &mut Custodian<T>,
        user: ID,
        quantity: u64,
    ) {
        increase_user_available_balance<T>(custodian, user, balance::create_for_testing(quantity));
    }

    #[test_only]
    public(friend) fun deposit<T>(
        custodian: &mut Custodian<T>,
        coin: Coin<T>,
        user: ID
    ) {
        increase_user_available_balance<T>(custodian, user, coin::into_balance(coin));
    }
}
