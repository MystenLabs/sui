// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

module sui_system::system_params_builder;

use sui_system::sui_system_state_inner::{Self, SystemParameters};

public struct Builder has drop {
    epoch_duration_ms: Option<u64>,
    stake_subsidy_start_epoch: Option<u64>,
    max_validator_count: Option<u64>,
    min_validator_joining_stake: Option<u64>,
    validator_low_stake_threshold: Option<u64>,
    validator_very_low_stake_threshold: Option<u64>,
    validator_low_stake_grace_period: Option<u64>,
}

public fun new(): Builder {
    Builder {
        epoch_duration_ms: option::none(),
        stake_subsidy_start_epoch: option::none(),
        max_validator_count: option::none(),
        min_validator_joining_stake: option::none(),
        validator_low_stake_threshold: option::none(),
        validator_very_low_stake_threshold: option::none(),
        validator_low_stake_grace_period: option::none(),
    }
}

public fun epoch_duration_ms(mut self: Builder, value: u64): Builder {
    self.epoch_duration_ms = option::some(value);
    self
}

public fun stake_subsidy_start_epoch(mut self: Builder, value: u64): Builder {
    self.stake_subsidy_start_epoch = option::some(value);
    self
}

public fun max_validator_count(mut self: Builder, value: u64): Builder {
    self.max_validator_count = option::some(value);
    self
}

public fun min_validator_joining_stake(mut self: Builder, value: u64): Builder {
    self.min_validator_joining_stake = option::some(value);
    self
}

public fun validator_low_stake_threshold(mut self: Builder, value: u64): Builder {
    self.validator_low_stake_threshold = option::some(value);
    self
}

public fun validator_very_low_stake_threshold(mut self: Builder, value: u64): Builder {
    self.validator_very_low_stake_threshold = option::some(value);
    self
}

public fun validator_low_stake_grace_period(mut self: Builder, value: u64): Builder {
    self.validator_low_stake_grace_period = option::some(value);
    self
}

public fun build(self: Builder, ctx: &mut TxContext): SystemParameters {
    let Builder {
        epoch_duration_ms,
        stake_subsidy_start_epoch,
        max_validator_count,
        min_validator_joining_stake,
        validator_low_stake_threshold,
        validator_very_low_stake_threshold,
        validator_low_stake_grace_period,
    } = self;

    sui_system_state_inner::create_system_parameters(
        epoch_duration_ms.destroy_or!(42),
        stake_subsidy_start_epoch.destroy_or!(0),
        max_validator_count.destroy_or!(100),
        min_validator_joining_stake.destroy_or!(1_000_000_000),
        validator_low_stake_threshold.destroy_or!(1_000_000_000),
        validator_very_low_stake_threshold.destroy_or!(1_000_000_000),
        validator_low_stake_grace_period.destroy_or!(7),
        ctx,
    )
}
