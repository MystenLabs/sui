// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

module sui_system::sui_system_state_inner {
    use sui::balance::{Self, Balance};
    use sui::sui::SUI;
    use sui::tx_context::TxContext;
    use sui::bag::{Self, Bag};
    use sui::table::{Self, Table};
    use sui::object::ID;

    use sui_system::validator::Validator;
    use sui_system::validator_wrapper::ValidatorWrapper;
    use sui_system::validator_wrapper;
    use sui_system::validator;
    use sui::object;

    const SYSTEM_STATE_VERSION_V1: u64 = 18446744073709551605;  // u64::MAX - 10

    public struct SystemParameters has store {
        epoch_duration_ms: u64,
        extra_fields: Bag,
    }

    public struct ValidatorSet has store {
        active_validators: vector<Validator>,
        inactive_validators: Table<ID, ValidatorWrapper>,
        extra_fields: Bag,
    }

    public struct SuiSystemStateInner has store {
        epoch: u64,
        protocol_version: u64,
        system_state_version: u64,
        validators: ValidatorSet,
        storage_fund: Balance<SUI>,
        parameters: SystemParameters,
        reference_gas_price: u64,
        safe_mode: bool,
        epoch_start_timestamp_ms: u64,
        extra_fields: Bag,
    }

    public(package) fun create(
        validators: vector<Validator>,
        storage_fund: Balance<SUI>,
        protocol_version: u64,
        epoch_start_timestamp_ms: u64,
        epoch_duration_ms: u64,
        ctx: &mut TxContext,
    ): SuiSystemStateInner {
        let validators = new_validator_set(validators, ctx);
        let mut system_state = SuiSystemStateInner {
            epoch: 0,
            protocol_version,
            system_state_version: genesis_system_state_version(),
            validators,
            storage_fund,
            parameters: SystemParameters {
                epoch_duration_ms,
                extra_fields: bag::new(ctx),
            },
            reference_gas_price: 1,
            safe_mode: false,
            epoch_start_timestamp_ms,
            extra_fields: bag::new(ctx),
        };
        // Add a dummy inactive validator so that we could test validator upgrade through wrapper latter.
        add_dummy_inactive_validator_for_testing(&mut system_state, ctx);
        system_state
    }

    public(package) fun advance_epoch(
        self: &mut SuiSystemStateInner,
        new_epoch: u64,
        next_protocol_version: u64,
        storage_reward: Balance<SUI>,
        computation_reward: Balance<SUI>,
        storage_rebate_amount: u64,
        epoch_start_timestamp_ms: u64,
    ) : Balance<SUI> {
        self.epoch_start_timestamp_ms = epoch_start_timestamp_ms;
        self.epoch = self.epoch + 1;
        assert!(new_epoch == self.epoch, 0);
        self.safe_mode = false;
        self.protocol_version = next_protocol_version;

        balance::join(&mut self.storage_fund, computation_reward);
        balance::join(&mut self.storage_fund, storage_reward);
        let storage_rebate = balance::split(&mut self.storage_fund, storage_rebate_amount);
        storage_rebate
    }

    public(package) fun protocol_version(self: &SuiSystemStateInner): u64 { self.protocol_version }
    public(package) fun system_state_version(self: &SuiSystemStateInner): u64 { self.system_state_version }
    public(package) fun genesis_system_state_version(): u64 {
        SYSTEM_STATE_VERSION_V1
    }

    public(package) fun add_dummy_inactive_validator_for_testing(self: &mut SuiSystemStateInner, ctx: &mut TxContext) {
        // Add a new entry to the inactive validator table for upgrade testing.
        let dummy_inactive_validator = validator_wrapper::create_v1(
            validator::new_dummy_inactive_validator(ctx),
            ctx,
        );
        table::add(&mut self.validators.inactive_validators, object::id_from_address(@0x0), dummy_inactive_validator);
    }

    fun new_validator_set(init_active_validators: vector<Validator>, ctx: &mut TxContext): ValidatorSet {
        ValidatorSet {
            active_validators: init_active_validators,
            inactive_validators: table::new(ctx),
            extra_fields: bag::new(ctx),
        }
    }
}
