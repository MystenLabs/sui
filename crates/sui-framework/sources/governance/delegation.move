// Copyright (c) 2022, Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

module sui::delegation {
    use std::option::{Self, Option};
    use sui::balance::{Self, Balance};
    use sui::coin::{Self, Coin};
    use sui::object::{Self, UID};
    use sui::locked_coin::{Self, LockedCoin};
    use sui::sui::SUI;
    use sui::transfer;
    use sui::tx_context::{Self, TxContext};
    use sui::epoch_time_lock::EpochTimeLock;

    friend sui::sui_system;

    /// A custodial delegation object. When the delegation is active, the delegation
    /// object holds the delegated stake coin. It also contains the delegation
    /// target validator address.
    /// The delegation object is required to claim delegation reward. The object
    /// keeps track of the next reward unclaimed epoch. One can only claim reward
    /// for the epoch that matches the `next_reward_unclaimed_epoch`.
    /// When the delegation is deactivated, we keep track of the ending epoch
    /// so that we know the ending epoch that the delegator can still claim reward.
    struct Delegation has key {
        id: UID,
        /// The delegated stake, if the delegate is still active
        active_delegation: Option<Balance<SUI>>,
        /// If the delegation is inactive, `ending_epoch` will be
        /// set to the ending epoch, i.e. the epoch when the delegation
        /// was withdrawn. Delegator will not be eligible to claim reward
        /// for ending_epoch and after.
        ending_epoch: Option<u64>,
        /// The delegated stake amount.
        delegate_amount: u64,
        /// Delegator is able to claim reward epoch by epoch. `next_reward_unclaimed_epoch`
        /// is the next epoch that the delegator can claim epoch. Whenever the delegator
        /// claims reward for an epoch, this value increments by one.
        next_reward_unclaimed_epoch: u64,
        /// The epoch until which the delegated coin is locked. If the delegated stake
        /// comes from a Coin<SUI>, this field is None. If it comes from a LockedCoin<SUI>, this
        /// field is not None, and after undelegation the stake will be returned to a LockedCoin<SUI>
        /// with locked_until_epoch set to this epoch.
        coin_locked_until_epoch: Option<EpochTimeLock>,
        /// The delegation target validator.
        validator_address: address,
    }

    public(friend) fun create(
        starting_epoch: u64,
        validator_address: address,
        stake: Coin<SUI>,
        ctx: &mut TxContext,
    ) {
        let delegate_amount = coin::value(&stake);
        let delegation = Delegation {
            id: object::new(ctx),
            active_delegation: option::some(coin::into_balance(stake)),
            ending_epoch: option::none(),
            delegate_amount,
            next_reward_unclaimed_epoch: starting_epoch,
            coin_locked_until_epoch: option::none(),
            validator_address,
        };
        transfer::transfer(delegation, tx_context::sender(ctx))
    }

    public(friend) fun create_from_locked_coin(
        starting_epoch: u64,
        validator_address: address,
        stake: LockedCoin<SUI>,
        ctx: &mut TxContext,
    ) {
        let delegate_amount = locked_coin::value(&stake);
        let (balance, epoch_lock) = locked_coin::into_balance(stake);
        let delegation = Delegation {
            id: object::new(ctx),
            active_delegation: option::some(balance),
            ending_epoch: option::none(),
            delegate_amount,
            next_reward_unclaimed_epoch: starting_epoch,
            coin_locked_until_epoch: option::some(epoch_lock),
            validator_address,
        };
        transfer::transfer(delegation, tx_context::sender(ctx))
    }

    /// Deactivate the delegation. Send back the stake and set the ending epoch.
    public(friend) fun undelegate(
        self: &mut Delegation,
        ending_epoch: u64,
        ctx: &mut TxContext,
    ) {
        assert!(is_active(self), 0);
        assert!(ending_epoch >= self.next_reward_unclaimed_epoch, 0);

        let stake = option::extract(&mut self.active_delegation);
        let sender = tx_context::sender(ctx);

        if (option::is_none(&self.coin_locked_until_epoch)) {
            transfer::transfer(coin::from_balance(stake, ctx), sender);
        } else {
            let locked_until_epoch = option::extract(&mut self.coin_locked_until_epoch);
            locked_coin::new_from_balance(stake, locked_until_epoch, sender, ctx);
        };

        self.ending_epoch = option::some(ending_epoch);
    }

    /// Switch the delegation from the current validator to `new_validator_address`.
    /// The current `Delegation` object `self` becomes inactive and the balance inside is
    /// extracted to the new `Delegation` object.
    public(friend) fun switch_delegation(
        self: &mut Delegation,
        new_validator_address: address,
        ctx: &mut TxContext,
    ) {
        assert!(is_active(self), 0);
        let current_epoch = tx_context::epoch(ctx);
        let balance = option::extract(&mut self.active_delegation);
        let delegate_amount = balance::value(&balance);

        let new_epoch_lock =
            if (option::is_some(&self.coin_locked_until_epoch)) {
                option::some(option::extract(&mut self.coin_locked_until_epoch))
            } else {
                option::none()
            };

        self.ending_epoch = option::some(current_epoch);

        let new_delegation = Delegation {
            id: object::new(ctx),
            active_delegation: option::some(balance),
            ending_epoch: option::none(),
            delegate_amount,
            next_reward_unclaimed_epoch: current_epoch + 1,
            coin_locked_until_epoch: new_epoch_lock,
            validator_address: new_validator_address,
        };
        transfer::transfer(new_delegation, tx_context::sender(ctx))
    }

    /// Claim delegation reward. Increment next_reward_unclaimed_epoch.
    public(friend) fun claim_reward(
        self: &mut Delegation,
        reward: Balance<SUI>,
        ctx: &mut TxContext,
    ) {
        let sender = tx_context::sender(ctx);
        coin::transfer(coin::from_balance(reward, ctx), sender);
        self.next_reward_unclaimed_epoch = self.next_reward_unclaimed_epoch + 1;
    }


    /// Destroy the delegation object. This can be done only when the delegation
    /// is inactive and all reward have been claimed.
    public entry fun burn(self: Delegation) {
        assert!(!is_active(&self), 0);

        let Delegation {
            id,
            active_delegation,
            ending_epoch,
            delegate_amount: _,
            next_reward_unclaimed_epoch,
            coin_locked_until_epoch,
            validator_address: _,
        } = self;
        object::delete(id);
        option::destroy_none(active_delegation);
        option::destroy_none(coin_locked_until_epoch);
        let ending_epoch = *option::borrow(&ending_epoch);
        assert!(next_reward_unclaimed_epoch == ending_epoch, 0);
    }

    public entry fun transfer(self: Delegation, recipient: address) {
        transfer::transfer(self, recipient)
    }

    /// Checks whether the delegation object is eligible to claim the reward
    /// given the epoch to claim and the validator address.
    public fun can_claim_reward(
        self: &Delegation,
        epoch_to_claim: u64,
        validator: address,
    ): bool {
        if (validator != self.validator_address || 
            self.next_reward_unclaimed_epoch > epoch_to_claim) 
        {
            return false
        }; 
        if (!is_active(self)) {
            let ending_epoch = *option::borrow(&self.ending_epoch);
            return ending_epoch > epoch_to_claim
        };
        true
    }

    public fun validator(self: &Delegation): address {
        self.validator_address
    }

    public fun delegate_amount(self: &Delegation): u64 {
        self.delegate_amount
    }

    public fun is_active(self: &Delegation): bool {
        option::is_some(&self.active_delegation) && option::is_none(&self.ending_epoch)
    }
}
