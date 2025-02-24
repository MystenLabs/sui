// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

module deepbook::custodian {
    use sui::balance::{Self, Balance, split};
    use sui::coin::{Self, Coin};
    use sui::table::{Self, Table};

    // <<<<<<<<<<<<<<<<<<<<<<<< Error codes <<<<<<<<<<<<<<<<<<<<<<<<

    public struct Account<phantom T> has store {
        available_balance: Balance<T>,
        locked_balance: Balance<T>,
    }

    public struct AccountCap has key, store { id: UID }

    // Custodian for limit orders.
    public struct Custodian<phantom T> has key, store {
        id: UID,
        /// Map from an AccountCap object ID to an Account object
        account_balances: Table<ID, Account<T>>,
    }

    #[deprecated(note = b"Minting account cap is deprecated. Please use Deepbook V3.")]
    /// Create an `AccountCap` that can be used across all DeepBook pool
    public fun mint_account_cap(_ctx: &mut TxContext): AccountCap {
        
        abort 1337
    }

    public(package) fun account_balance<Asset>(
        custodian: &Custodian<Asset>,
        user: ID
    ): (u64, u64) {
        // if custodian account is not created yet, directly return (0, 0) rather than abort
        if (!table::contains(&custodian.account_balances, user)) {
            return (0, 0)
        };
        let account_balances = table::borrow(&custodian.account_balances, user);
        let avail_balance = balance::value(&account_balances.available_balance);
        let locked_balance = balance::value(&account_balances.locked_balance);
        (avail_balance, locked_balance)
    }

    public(package) fun new<T>(ctx: &mut TxContext): Custodian<T> {
        Custodian<T> {
            id: object::new(ctx),
            account_balances: table::new(ctx),
        }
    }

    public(package) fun withdraw_asset<Asset>(
        custodian: &mut Custodian<Asset>,
        quantity: u64,
        account_cap: &AccountCap,
        ctx: &mut TxContext
    ): Coin<Asset> {
        coin::from_balance(decrease_user_available_balance<Asset>(custodian, account_cap, quantity), ctx)
    }

    public(package) fun increase_user_available_balance<T>(
        custodian: &mut Custodian<T>,
        user: ID,
        quantity: Balance<T>,
    ) {
        let account = borrow_mut_account_balance<T>(custodian, user);
        balance::join(&mut account.available_balance, quantity);
    }

    public(package) fun decrease_user_available_balance<T>(
        custodian: &mut Custodian<T>,
        account_cap: &AccountCap,
        quantity: u64,
    ): Balance<T> {
        let account = borrow_mut_account_balance<T>(custodian, object::uid_to_inner(&account_cap.id));
        balance::split(&mut account.available_balance, quantity)
    }

    public(package) fun increase_user_locked_balance<T>(
        custodian: &mut Custodian<T>,
        account_cap: &AccountCap,
        quantity: Balance<T>,
    ) {
        let account = borrow_mut_account_balance<T>(custodian, object::uid_to_inner(&account_cap.id));
        balance::join(&mut account.locked_balance, quantity);
    }

    public(package) fun decrease_user_locked_balance<T>(
        custodian: &mut Custodian<T>,
        user: ID,
        quantity: u64,
    ): Balance<T> {
        let account = borrow_mut_account_balance<T>(custodian, user);
        split(&mut account.locked_balance, quantity)
    }

    /// Move `quantity` from the unlocked balance of `user` to the locked balance of `user`
    public(package) fun lock_balance<T>(
        custodian: &mut Custodian<T>,
        account_cap: &AccountCap,
        quantity: u64,
    ) {
        let to_lock = decrease_user_available_balance(custodian, account_cap, quantity);
        increase_user_locked_balance(custodian, account_cap, to_lock);
    }

    /// Move `quantity` from the locked balance of `user` to the unlocked balacne of `user`
    public(package) fun unlock_balance<T>(
        custodian: &mut Custodian<T>,
        user: ID,
        quantity: u64,
    ) {
        let locked_balance = decrease_user_locked_balance<T>(custodian, user, quantity);
        increase_user_available_balance<T>(custodian, user, locked_balance)
    }

    public(package) fun account_available_balance<T>(
        custodian: &Custodian<T>,
        user: ID,
    ): u64 {
        balance::value(&table::borrow(&custodian.account_balances, user).available_balance)
    }

    public(package) fun account_locked_balance<T>(
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
}
