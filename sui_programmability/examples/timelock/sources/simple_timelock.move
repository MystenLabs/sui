// Copyright (c) 2022, Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

/// Simple timelock module for time-delayed transfers.
///
/// Lock funds that can only be withdrawn after a specific time.
/// Useful for vesting, scheduled payments, or security delays.
module timelock::simple_timelock {
    use sui::object::{Self, Info, UID};
    use sui::coin::{Self, Coin};
    use sui::balance::{Self, Balance};
    use sui::transfer;
    use sui::tx_context::{Self, TxContext};

    /// Error codes
    const ENotUnlocked: u64 = 0;
    const ENotRecipient: u64 = 1;
    const EInsufficientAmount: u64 = 2;

    /// A timelock holding funds until a specific time
    struct TimeLock<phantom T> has key {
        id: UID,
        locked_balance: Balance<T>,
        recipient: address,
        unlock_epoch: u64,
        sender: address,
    }

    /// Create a new timelock
    public entry fun create_lock<T>(
        funds: Coin<T>,
        recipient: address,
        unlock_epoch: u64,
        ctx: &mut TxContext
    ) {
        let lock = TimeLock<T> {
            id: object::new(ctx),
            locked_balance: coin::into_balance(funds),
            recipient,
            unlock_epoch,
            sender: tx_context::sender(ctx),
        };

        transfer::share_object(lock);
    }

    /// Withdraw funds after unlock time
    public entry fun withdraw<T>(
        lock: TimeLock<T>,
        ctx: &mut TxContext
    ) {
        let TimeLock {
            id,
            locked_balance,
            recipient,
            unlock_epoch,
            sender: _
        } = lock;

        // Check unlock time has passed
        assert!(tx_context::epoch(ctx) >= unlock_epoch, ENotUnlocked);

        // Check sender is the recipient
        assert!(tx_context::sender(ctx) == recipient, ENotRecipient);

        // Transfer funds to recipient
        let amount = balance::value(&locked_balance);
        let unlocked_coin = coin::take(&mut locked_balance, amount, ctx);
        balance::destroy_zero(locked_balance);

        transfer::transfer(unlocked_coin, recipient);
        object::delete(id);
    }

    /// Withdraw partial amount after unlock time
    public entry fun withdraw_partial<T>(
        lock: &mut TimeLock<T>,
        amount: u64,
        ctx: &mut TxContext
    ) {
        // Check unlock time has passed
        assert!(tx_context::epoch(ctx) >= lock.unlock_epoch, ENotUnlocked);

        // Check sender is the recipient
        assert!(tx_context::sender(ctx) == lock.recipient, ENotRecipient);

        // Check sufficient balance
        assert!(balance::value(&lock.locked_balance) >= amount, EInsufficientAmount);

        // Transfer partial amount to recipient
        let partial_coin = coin::take(&mut lock.locked_balance, amount, ctx);
        transfer::transfer(partial_coin, lock.recipient);
    }

    /// View functions

    /// Get timelock info
    public fun get_info<T>(lock: &TimeLock<T>): (address, address, u64, u64) {
        (
            lock.sender,
            lock.recipient,
            balance::value(&lock.locked_balance),
            lock.unlock_epoch
        )
    }

    /// Check if timelock is unlocked
    public fun is_unlocked<T>(lock: &TimeLock<T>, current_epoch: u64): bool {
        current_epoch >= lock.unlock_epoch
    }

    /// Get locked amount
    public fun locked_amount<T>(lock: &TimeLock<T>): u64 {
        balance::value(&lock.locked_balance)
    }

    /// Get time until unlock
    public fun epochs_until_unlock<T>(lock: &TimeLock<T>, current_epoch: u64): u64 {
        if (current_epoch >= lock.unlock_epoch) {
            0
        } else {
            lock.unlock_epoch - current_epoch
        }
    }

    /// Get recipient address
    public fun recipient<T>(lock: &TimeLock<T>): address {
        lock.recipient
    }

    /// Get sender address
    public fun sender<T>(lock: &TimeLock<T>): address {
        lock.sender
    }
}
