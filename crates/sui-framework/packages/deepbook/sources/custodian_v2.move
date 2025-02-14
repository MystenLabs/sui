// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

module deepbook::custodian_v2 {
    use sui::balance::{Self, Balance, split};
    use sui::coin::{Self, Coin};
    use sui::table::{Self, Table};

    // <<<<<<<<<<<<<<<<<<<<<<<< Error codes <<<<<<<<<<<<<<<<<<<<<<<<
    const EAdminAccountCapRequired: u64 = 2;
    // <<<<<<<<<<<<<<<<<<<<<<<< Error codes <<<<<<<<<<<<<<<<<<<<<<<<

    public struct Account<phantom T> has store {
        available_balance: Balance<T>,
        locked_balance: Balance<T>,
    }

    /// Capability granting permission to access an entry in `Custodian.account_balances`.
    /// Calling `mint_account_cap` creates an "admin account cap" such that id == owner with
    /// the permission to both access funds and create new `AccountCap`s.
    /// Calling `create_child_account_cap` creates a "child account cap" such that id != owner
    /// that can access funds, but cannot create new `AccountCap`s.
    public struct AccountCap has key, store {
        id: UID,
        /// The owner of this AccountCap. Note: this is
        /// derived from an object ID, not a user address
        owner: address
    }

    // Custodian for limit orders.
    public struct Custodian<phantom T> has key, store {
        id: UID,
        /// Map from the owner address of AccountCap object to an Account object
        account_balances: Table<address, Account<T>>,
    }

    /// Create a "child account cap" such that id != owner
    /// that can access funds, but cannot create new `AccountCap`s.
    public fun create_child_account_cap(admin_account_cap: &AccountCap, ctx: &mut TxContext): AccountCap {
        // Only the admin account cap can create new account caps
        assert!(object::uid_to_address(&admin_account_cap.id) == admin_account_cap.owner, EAdminAccountCapRequired);

        AccountCap {
            id: object::new(ctx),
            owner: admin_account_cap.owner
        }
    }

    /// Destroy the given `account_cap` object
    public fun delete_account_cap(account_cap: AccountCap) {
        let AccountCap { id, owner: _ } = account_cap;
        object::delete(id)
    }

    /// Return the owner of an AccountCap
    public fun account_owner(account_cap: &AccountCap): address {
        account_cap.owner
    }

    public(package) fun account_balance<Asset>(
        custodian: &Custodian<Asset>,
        owner: address
    ): (u64, u64) {
        // if custodian account is not created yet, directly return (0, 0) rather than abort
        if (!table::contains(&custodian.account_balances, owner)) {
            return (0, 0)
        };
        let account_balances = table::borrow(&custodian.account_balances, owner);
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
        owner: address,
        quantity: Balance<T>,
    ) {
        let account = borrow_mut_account_balance<T>(custodian, owner);
        balance::join(&mut account.available_balance, quantity);
    }

    public(package) fun decrease_user_available_balance<T>(
        custodian: &mut Custodian<T>,
        account_cap: &AccountCap,
        quantity: u64,
    ): Balance<T> {
        let account = borrow_mut_account_balance<T>(custodian, account_cap.owner);
        balance::split(&mut account.available_balance, quantity)
    }

    public(package) fun increase_user_locked_balance<T>(
        custodian: &mut Custodian<T>,
        account_cap: &AccountCap,
        quantity: Balance<T>,
    ) {
        let account = borrow_mut_account_balance<T>(custodian, account_cap.owner);
        balance::join(&mut account.locked_balance, quantity);
    }

    public(package) fun decrease_user_locked_balance<T>(
        custodian: &mut Custodian<T>,
        owner: address,
        quantity: u64,
    ): Balance<T> {
        let account = borrow_mut_account_balance<T>(custodian, owner);
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

    /// Move `quantity` from the locked balance of `user` to the unlocked balance of `user`
    public(package) fun unlock_balance<T>(
        custodian: &mut Custodian<T>,
        owner: address,
        quantity: u64,
    ) {
        let locked_balance = decrease_user_locked_balance<T>(custodian, owner, quantity);
        increase_user_available_balance<T>(custodian, owner, locked_balance)
    }

    public(package) fun account_available_balance<T>(
        custodian: &Custodian<T>,
        owner: address,
    ): u64 {
        balance::value(&table::borrow(&custodian.account_balances, owner).available_balance)
    }

    public(package) fun account_locked_balance<T>(
        custodian: &Custodian<T>,
        owner: address,
    ): u64 {
        balance::value(&table::borrow(&custodian.account_balances, owner).locked_balance)
    }

    fun borrow_mut_account_balance<T>(
        custodian: &mut Custodian<T>,
        owner: address,
    ): &mut Account<T> {
        if (!table::contains(&custodian.account_balances, owner)) {
            table::add(
                &mut custodian.account_balances,
                owner,
                Account { available_balance: balance::zero(), locked_balance: balance::zero() }
            );
        };
        table::borrow_mut(&mut custodian.account_balances, owner)
    }
}
