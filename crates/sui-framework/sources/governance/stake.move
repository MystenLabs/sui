// Copyright (c) 2022, Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

module sui::stake {
    use std::option::{Self, Option};
    use sui::balance::Balance;
    use sui::id::VersionedID;
    use sui::locked_coin;
    use sui::sui::SUI;
    use sui::transfer;
    use sui::tx_context::{Self, TxContext};
    use sui::epoch_time_lock::EpochTimeLock;
    use sui::epoch_time_lock;
    use sui::balance;
    use sui::math;

    friend sui::sui_system;
    friend sui::validator;

    /// A custodial stake object holding the staked SUI coin.
    struct Stake has key {
        id: VersionedID,
        /// The staked SUI tokens.
        balance: Balance<SUI>,
        /// The epoch until which the staked coin is locked. If the stake
        /// comes from a Coin<SUI>, this field is None. If it comes from a LockedCoin<SUI>, this
        /// field will record the original lock expiration epoch, to be used when unstaking.
        coin_locked_until_epoch: Option<EpochTimeLock>,
    }

    /// The number of epochs the withdrawn stake is locked for.
    /// TODO: this is a placehodler number and may be changed.
    const BONDING_PERIOD: u64 = 1;

    /// Create a stake object from a SUI balance. If the balance comes from a
    /// `LockedCoin`, an EpochTimeLock is passed in to keep track of locking period.
    public(friend) fun create(
        balance: Balance<SUI>,
        recipient: address,
        coin_locked_until_epoch: Option<EpochTimeLock>,
        ctx: &mut TxContext,
    ) {
        let stake = Stake {
            id: tx_context::new_id(ctx),
            balance,
            coin_locked_until_epoch,
        };
        transfer::transfer(stake, recipient)
    }

    /// Withdraw `amount` from the balance of `stake`.
    public(friend) fun withdraw_stake(
        self: &mut Stake,
        amount: u64,
        ctx: &mut TxContext,
    ) {
        let sender = tx_context::sender(ctx);
        let unlock_epoch = tx_context::epoch(ctx) + BONDING_PERIOD;
        let balance = balance::split(&mut self.balance, amount);

        if (option::is_none(&self.coin_locked_until_epoch)) {
            // If the stake didn't come from a locked coin, we give back the stake and
            // lock the coin for `BONDING_PERIOD`.
            locked_coin::new_from_balance(balance, epoch_time_lock::new(unlock_epoch, ctx), sender, ctx);
        } else {
            // If the stake did come from a locked coin, we lock the coin for
            // max(BONDING_PERIOD, remaining_lock_time).
            let original_unlock_epoch = epoch_time_lock::epoch(option::borrow(&self.coin_locked_until_epoch));
            let unlock_epoch = math::max(original_unlock_epoch, unlock_epoch);
            locked_coin::new_from_balance(balance, epoch_time_lock::new(unlock_epoch, ctx), sender, ctx);
        };
    }

    public fun value(self: &Stake): u64 {
        balance::value(&self.balance)
    }
}
