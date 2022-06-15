// Copyright (c) 2022, Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

module Sui::LockedCoin {
    use Sui::Balance::{Self, Balance};
    use Sui::Coin::{Self, Coin};
    use Sui::ID::{Self, VersionedID};
    use Sui::Transfer;
    use Sui::TxContext::{Self, TxContext};
    use Sui::EpochTimeLock::{Self, EpochTimeLock};

    friend Sui::Delegation;

    /// A coin of type `T` locked until `locked_until_epoch`.
    struct LockedCoin<phantom T> has key, store {
        id: VersionedID,
        balance: Balance<T>,
        locked_until_epoch: EpochTimeLock
    }

    /// Create a LockedCoin from `balance` and transfer it to `owner`.
    public fun new_from_balance<T>(balance: Balance<T>, locked_until_epoch: EpochTimeLock, owner: address, ctx: &mut TxContext) {
        let locked_coin = LockedCoin {
            id: TxContext::new_id(ctx),
            balance,
            locked_until_epoch
        };
        Transfer::transfer(locked_coin, owner);
    }

    /// Destruct a LockedCoin wrapper and keep the balance.
    public(friend) fun into_balance<T>(coin: LockedCoin<T>): (Balance<T>, EpochTimeLock) {
        let LockedCoin { id, locked_until_epoch, balance } = coin;
        ID::delete(id);
        (balance, locked_until_epoch)
    }

    /// Public getter for the locked coin's value
    public fun value<T>(self: &LockedCoin<T>): u64 {
        Balance::value(&self.balance)
    }

    /// Lock a coin up until `locked_until_epoch`. The input Coin<T> is deleted and a LockedCoin<T>
    /// is transferred to the `recipient`. This function aborts if the `locked_until_epoch` is less than
    /// or equal to the current epoch.
    public entry fun lock_coin<T>(
        coin: Coin<T>, recipient: address, locked_until_epoch: u64, ctx: &mut TxContext
    ) {
        let balance = Coin::into_balance(coin);
        new_from_balance(balance, EpochTimeLock::new(locked_until_epoch, ctx), recipient, ctx);
    }

    /// Unlock a locked coin. The function aborts if the current epoch is less than the `locked_until_epoch`
    /// of the coin. If the check is successful, the locked coin is deleted and a Coin<T> is transferred back
    /// to the sender.
    public entry fun unlock_coin<T>(locked_coin: LockedCoin<T>, ctx: &mut TxContext) {
        let LockedCoin { id, balance, locked_until_epoch } = locked_coin;
        ID::delete(id);
        EpochTimeLock::destroy(locked_until_epoch, ctx);
        let coin = Coin::from_balance(balance, ctx);
        Transfer::transfer(coin, TxContext::sender(ctx));
    }
}
