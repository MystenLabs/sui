// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

module sui::sui_system {
    use sui::balance::Balance;
    use sui::object::UID;
    use sui::sui::SUI;
    use sui::transfer;
    use sui::tx_context::{Self, TxContext};
    use sui::validator::Validator;
    use sui::sui_system_state_inner::SuiSystemStateInner;
    use sui::dynamic_field;
    use sui::sui_system_state_inner;

    friend sui::genesis;

    struct SuiSystemState has key {
        id: UID,
        version: u64,
    }

    // ==== functions that can only be called by genesis ====

    /// Create a new SuiSystemState object and make it shared.
    /// This function will be called only once in genesis.
    public(friend) fun create(
        id: UID,
        validators: vector<Validator>,
        stake_subsidy_fund: Balance<SUI>,
        storage_fund: Balance<SUI>,
        protocol_version: u64,
        system_state_version: u64,
        governance_start_epoch: u64,
        epoch_start_timestamp_ms: u64,
        epoch_duration_ms: u64,
        initial_stake_subsidy_distribution_amount: u64,
        stake_subsidy_period_length: u64,
        stake_subsidy_decrease_rate: u16,
        ctx: &mut TxContext,
    ) {
        let system_state = sui_system_state_inner::create(
            validators,
            stake_subsidy_fund,
            storage_fund,
            protocol_version,
            system_state_version,
            governance_start_epoch,
            epoch_start_timestamp_ms,
            epoch_duration_ms,
            initial_stake_subsidy_distribution_amount,
            stake_subsidy_period_length,
            stake_subsidy_decrease_rate,
            ctx,
        );
        let self = SuiSystemState {
            id,
            version: system_state_version,
        };
        dynamic_field::add(&mut self.id, system_state_version, system_state);
        transfer::share_object(self);
    }

    fun advance_epoch(
        storage_reward: Balance<SUI>,
        computation_reward: Balance<SUI>,
        wrapper: &mut SuiSystemState,
        new_epoch: u64,
        next_protocol_version: u64,
        storage_rebate: u64,
        _storage_fund_reinvest_rate: u64, // share of storage fund's rewards that's reinvested
                                         // into storage fund, in basis point.
        _reward_slashing_rate: u64, // how much rewards are slashed to punish a validator, in bps.
        epoch_start_timestamp_ms: u64, // Timestamp of the epoch start
        new_system_state_version: u64,
        ctx: &mut TxContext,
    ) : Balance<SUI> {
        let self = load_system_state_mut(wrapper);
        // Validator will make a special system call with sender set as 0x0.
        assert!(tx_context::sender(ctx) == @0x0, 0);
        let old_protocol_version = sui_system_state_inner::protocol_version(self);
        let storage_rebate = sui_system_state_inner::advance_epoch(
            self,
            new_epoch,
            next_protocol_version,
            storage_reward,
            computation_reward,
            storage_rebate,
            epoch_start_timestamp_ms,
        );

        if (new_system_state_version != wrapper.version) {
            // If we are upgrading the system state, we need to make sure that the protocol version
            // is also upgraded.
            assert!(old_protocol_version != next_protocol_version, 0);
            let cur_state: SuiSystemStateInner = dynamic_field::remove(&mut wrapper.id, wrapper.version);
            let new_state = sui_system_state_inner::upgrade_system_state(cur_state, new_system_state_version, ctx);
            wrapper.version = new_system_state_version;
            dynamic_field::add(&mut wrapper.id, wrapper.version, new_state);
        };
        storage_rebate
    }

    fun load_system_state(self: &SuiSystemState): &SuiSystemStateInner {
        let version = self.version;
        let inner: &SuiSystemStateInner = dynamic_field::borrow(&self.id, version);
        assert!(sui_system_state_inner::system_state_version(inner) == version, 0);
        inner
    }

    fun load_system_state_mut(self: &mut SuiSystemState): &mut SuiSystemStateInner {
        let version = self.version;
        let inner: &mut SuiSystemStateInner = dynamic_field::borrow_mut(&mut self.id, version);
        assert!(sui_system_state_inner::system_state_version(inner) == version, 0);
        inner
    }
}
