// Copyright (c) 2022, Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

module Sui::Validator {
    use Std::ASCII::{Self, String};
    use Std::Option::{Self, Option};
    use Std::Vector;

    use Sui::Balance::{Self, Balance};
    use Sui::Coin;
    use Sui::SUI::SUI;
    use Sui::Transfer;
    use Sui::TxContext::TxContext;

    friend Sui::Genesis;
    friend Sui::SuiSystem;
    friend Sui::ValidatorSet;

    #[test_only]
    friend Sui::ValidatorTests;
    #[test_only]
    friend Sui::ValidatorSetTests;

    struct Validator has store {
        /// The Sui Address of the validator. This is the sender that created the Validator object,
        /// and also the address to send validator/coins to during withdraws.
        sui_address: address,
        /// A unique human-readable name of this validator.
        name: String,
        /// The network address of the validator (could also contain extra info such as port, DNS and etc.).
        net_address: vector<u8>,
        /// The current active stake. This will not change during an epoch. It can only
        /// be updated at the end of epoch.
        stake: Balance<SUI>,
        /// Pending stake deposits. It will be put into `stake` at the end of epoch.
        pending_stake: Option<Balance<SUI>>,
        /// Pending withdraw amount, processed at end of epoch.
        pending_withdraw: u64,
    }

    public(friend) fun new(
        sui_address: address,
        name: vector<u8>,
        net_address: vector<u8>,
        stake: Balance<SUI>,
    ): Validator {
        assert!(
            Vector::length(&net_address) <= 100 || Vector::length(&name) <= 50,
            0
        );
        Validator {
            sui_address,
            name: ASCII::string(name),
            net_address,
            stake,
            pending_stake: Option::none(),
            pending_withdraw: 0,
        }
    }

    public(friend) fun destroy(self: Validator, ctx: &mut TxContext) {
        let Validator {
            sui_address,
            name: _,
            net_address: _,
            stake,
            pending_stake,
            pending_withdraw,
        } = self;

        Transfer::transfer(Coin::from_balance(stake, ctx), sui_address);
        assert!(pending_withdraw == 0 && Option::is_none(&pending_stake), 0);
        Option::destroy_none(pending_stake);
}

    /// Add stake to an active validator. The new stake is added to the pending_stake field,
    /// which will be processed at the end of epoch.
    public(friend) fun request_add_stake(
        self: &mut Validator,
        new_stake: Balance<SUI>,
        max_validator_stake: u64,
    ) {
        let cur_stake = Balance::value(&self.stake);
        if (Option::is_none(&self.pending_stake)) {
            assert!(
                cur_stake + Balance::value(&new_stake) <= max_validator_stake,
                0
            );
            Option::fill(&mut self.pending_stake, new_stake)
        } else {
            let pending_stake = Option::extract(&mut self.pending_stake);
            Balance::join(&mut pending_stake, new_stake);
            assert!(
                cur_stake + Balance::value(&pending_stake) <= max_validator_stake,
                0
            );
            Option::fill(&mut self.pending_stake, pending_stake);
        }
    }

    /// Withdraw stake from an active validator. Since it's active, we need
    /// to add it to the pending withdraw amount and process it at the end
    /// of epoch. We also need to make sure there is sufficient amount to withdraw.
    public(friend) fun request_withdraw_stake(
        self: &mut Validator,
        withdraw_amount: u64,
        min_validator_stake: u64,
    ) {
        self.pending_withdraw = self.pending_withdraw + withdraw_amount;

        let pending_stake_amount = if (Option::is_none(&self.pending_stake)) {
            0
        } else {
            Balance::value(Option::borrow(&self.pending_stake))
        };
        let total_stake = Balance::value(&self.stake) + pending_stake_amount;
        assert!(total_stake >= self.pending_withdraw + min_validator_stake, 0);
    }

    /// Process pending stake and pending withdraws.
    public(friend) fun adjust_stake(self: &mut Validator, ctx: &mut TxContext) {
        if (Option::is_some(&self.pending_stake)) {
            let pending_stake = Option::extract(&mut self.pending_stake);
            Balance::join(&mut self.stake, pending_stake);
        };
        if (self.pending_withdraw > 0) {
            let coin = Coin::withdraw(&mut self.stake, self.pending_withdraw, ctx);
            Coin::transfer(coin, self.sui_address);
            self.pending_withdraw = 0;
        }
    }


    public fun sui_address(self: &Validator): address {
        self.sui_address
    }

    public fun stake_amount(self: &Validator): u64 {
        Balance::value(&self.stake)
    }

    public fun pending_stake_amount(self: &Validator): u64 {
        if (Option::is_some(&self.pending_stake)) {
            Balance::value(Option::borrow(&self.pending_stake))
        } else {
            0
        }
    }

    public fun pending_withdraw(self: &Validator): u64 {
        self.pending_withdraw
    }

    public fun is_duplicate(self: &Validator, other: &Validator): bool {
         self.sui_address == other.sui_address
            || self.name == other.name
            || self.net_address == other.net_address
    }
}
