// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

module utils::locked_coin {
    use sui::balance::Balance;
    use sui::coin::{Self, Coin};
    use utils::epoch_time_lock::{Self, EpochTimeLock};

    /// A coin of type `T` locked until `locked_until_epoch`.
    public struct LockedCoin<phantom T> has key {
        id: UID,
        balance: Balance<T>,
        locked_until_epoch: EpochTimeLock
    }

    /// Create a LockedCoin from `balance` and transfer it to `owner`.
    public fun new_from_balance<T>(balance: Balance<T>, locked_until_epoch: EpochTimeLock, owner: address, ctx: &mut TxContext) {
        let locked_coin = LockedCoin {
            id: object::new(ctx),
            balance,
            locked_until_epoch
        };
        transfer::transfer(locked_coin, owner);
    }

    /// Public getter for the locked coin's value
    public fun value<T>(self: &LockedCoin<T>): u64 {
        self.balance.value()
    }

    /// Lock a coin up until `locked_until_epoch`. The input Coin<T> is deleted and a LockedCoin<T>
    /// is transferred to the `recipient`. This function aborts if the `locked_until_epoch` is less than
    /// or equal to the current epoch.
    public entry fun lock_coin<T>(
        coin: Coin<T>, recipient: address, locked_until_epoch: u64, ctx: &mut TxContext
    ) {
        let balance = coin.into_balance();
        new_from_balance(balance, epoch_time_lock::new(locked_until_epoch, ctx), recipient, ctx);
    }

    /// Unlock a locked coin. The function aborts if the current epoch is less than the `locked_until_epoch`
    /// of the coin. If the check is successful, the locked coin is deleted and a Coin<T> is transferred back
    /// to the sender.
    public entry fun unlock_coin<T>(locked_coin: LockedCoin<T>, ctx: &mut TxContext) {
        let LockedCoin { id, balance, locked_until_epoch } = locked_coin;
        id.delete();
        locked_until_epoch.destroy(ctx);
        let coin = coin::from_balance(balance, ctx);
        transfer::public_transfer(coin, ctx.sender());
    }
}
