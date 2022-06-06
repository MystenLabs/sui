// Copyright (c) 2022, Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

module Sui::LockedCoin {
    use Sui::Balance::{Self, Balance};
    use Sui::Coin::{Self, Coin};
    use Std::Errors;
    use Sui::ID::{Self, VersionedID};
    use Sui::Transfer;
    use Sui::TxContext::{Self, TxContext};

    /// The locked_until time passed into the creation of a locked coin is invalid.
    const EINVALID_LOCK_UNTIL: u64 = 0;
    /// Attempt is made to unlock a locked coin that has not been unlocked yet.
    const ECOIN_STILL_LOCKED: u64 = 1;

    /// A coin of type `T` locked until `locked_until_epoch`.
    struct LockedCoin<phantom T> has key, store {
        id: VersionedID,
        balance: Balance<T>,
        locked_until_epoch: u64
    }

    /// Returns the epoch until which the coin is locked.
    public fun locked_until_epoch<T>(locked_coin: &LockedCoin<T>) : u64 {
        locked_coin.locked_until_epoch
    }

    /// Wrap a balance into a LockedCoin.
    public fun from_balance<T>(balance: Balance<T>, locked_until_epoch: u64, ctx: &mut TxContext): LockedCoin<T> {
        LockedCoin { id: TxContext::new_id(ctx), balance, locked_until_epoch }
    }

    /// Destruct a LockedCoin wrapper and keep the balance.
    public fun into_balance<T>(coin: LockedCoin<T>): Balance<T> {
        let LockedCoin { id, locked_until_epoch: _, balance } = coin;
        ID::delete(id);
        balance
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
        assert!(TxContext::epoch(ctx) < locked_until_epoch, Errors::invalid_argument(EINVALID_LOCK_UNTIL));
        let balance = Coin::into_balance(coin);
        let locked_coin = LockedCoin { id: TxContext::new_id(ctx), balance, locked_until_epoch };
        Transfer::transfer(locked_coin, recipient);
    }

    /// Unlock a locked coin. The function aborts if the current epoch is less than the `locked_until_epoch`
    /// of the coin. If the check is successful, the locked coin is deleted and a Coin<T> is transferred back
    /// to the sender.
    public entry fun unlock_coin<T>(locked_coin: LockedCoin<T>, ctx: &mut TxContext) {
        let LockedCoin { id, balance, locked_until_epoch } = locked_coin;
        assert!(TxContext::epoch(ctx) >= locked_until_epoch, Errors::invalid_argument(ECOIN_STILL_LOCKED));
        ID::delete(id);
        let coin = Coin::from_balance(balance, ctx);
        Transfer::transfer(coin, TxContext::sender(ctx));
    }
}
