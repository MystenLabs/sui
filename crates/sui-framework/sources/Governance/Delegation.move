// Copyright (c) 2022, Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

module Sui::Delegation {
    use Std::Option::{Self, Option};
    use Sui::Balance::Balance;
    use Sui::Coin::{Self, Coin};
    use Sui::ID::{Self, VersionedID};
    use Sui::SUI::SUI;
    use Sui::Transfer;
    use Sui::TxContext::{Self, TxContext};

    friend Sui::SuiSystem;

    /// A custodial delegation object. When the delegation is active, the delegation
    /// object holds the delegated stake coin. It also contains the delegation
    /// target validator address.
    /// The delegation object is required to claim delegation reward. The object
    /// keeps track of the next reward unclaimed epoch. One can only claim reward
    /// for the epoch that matches the `next_reward_unclaimed_epoch`.
    /// When the delegation is deactivated, we keep track of the ending epoch
    /// so that we know the ending epoch that the delegator can still claim reward.
    struct Delegation has key {
        id: VersionedID,
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
        /// The delegation target validator.
        validator_address: address,
    }

    public(friend) fun create(
        starting_epoch: u64,
        validator_address: address,
        stake: Coin<SUI>,
        ctx: &mut TxContext,
    ) {
        let delegate_amount = Coin::value(&stake);
        let delegation = Delegation {
            id: TxContext::new_id(ctx),
            active_delegation: Option::some(Coin::into_balance(stake)),
            ending_epoch: Option::none(),
            delegate_amount,
            next_reward_unclaimed_epoch: starting_epoch,
            validator_address,
        };
        Transfer::transfer(delegation, TxContext::sender(ctx))
    }

    /// Deactivate the delegation. Send back the stake and set the ending epoch.
    public(friend) fun undelegate(
        self: &mut Delegation,
        ending_epoch: u64,
        ctx: &mut TxContext,
    ) {
        assert!(is_active(self), 0);
        assert!(ending_epoch >= self.next_reward_unclaimed_epoch, 0);

        let stake = Option::extract(&mut self.active_delegation);
        let sender = TxContext::sender(ctx);
        Transfer::transfer(Coin::from_balance(stake, ctx), sender);

        self.ending_epoch = Option::some(ending_epoch);
    }

    /// Claim delegation reward. Increment next_reward_unclaimed_epoch.
    public(friend) fun claim_reward(
        self: &mut Delegation,
        reward: Balance<SUI>,
        ctx: &mut TxContext,
    ) {
        let sender = TxContext::sender(ctx);
        Coin::transfer(Coin::from_balance(reward, ctx), sender);
        self.next_reward_unclaimed_epoch = self.next_reward_unclaimed_epoch + 1;
    }


    /// Destroy the delegation object. This can be done only when the delegation
    /// is inactive and all reward have been claimed.
    public(script) fun burn(self: Delegation) {
        assert!(!is_active(&self), 0);

        let Delegation {
            id,
            active_delegation,
            ending_epoch,
            delegate_amount: _,
            next_reward_unclaimed_epoch,
            validator_address: _,
        } = self;
        ID::delete(id);
        Option::destroy_none(active_delegation);
        let ending_epoch = *Option::borrow(&ending_epoch);
        assert!(next_reward_unclaimed_epoch == ending_epoch, 0);
    }

    public(script) fun transfer(self: Delegation, recipient: address) {
        Transfer::transfer(self, recipient)
    }

    /// Checks whether the delegation object is eligible to claim the reward
    /// given the epoch to claim and the validator address.
    public fun can_claim_reward(
        self: &Delegation,
        epoch_to_claim: u64,
        validator: address,
    ): bool {
        if (validator != self.validator_address) {
            false
        } else if (is_active(self)) {
            self.next_reward_unclaimed_epoch <= epoch_to_claim
        } else {
            let ending_epoch = *Option::borrow(&self.ending_epoch);
            ending_epoch > epoch_to_claim
        }
    }

    public fun validator(self: &Delegation): address {
        self.validator_address
    }

    public fun delegate_amount(self: &Delegation): u64 {
        self.delegate_amount
    }

    fun is_active(self: &Delegation): bool {
        Option::is_some(&self.active_delegation) && Option::is_none(&self.ending_epoch)
    }
}
