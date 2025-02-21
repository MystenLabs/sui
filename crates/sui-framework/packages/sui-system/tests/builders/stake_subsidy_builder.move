// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

module sui_system::stake_subsidy_builder;

use sui::balance::{Self, Balance};
use sui::sui::SUI;

public struct Builder {
    balance: Option<Balance<SUI>>,
    distribution_counter: Option<u64>,
    current_distribution_amount: Option<u64>,
    stake_subsidy_period_length: Option<u64>,
    stake_subsidy_decrease_rate: Option<u16>,
}

public fun new(): Builder {
    Builder {
        balance: option::none(),
        distribution_counter: option::none(),
        current_distribution_amount: option::none(),
        stake_subsidy_period_length: option::none(),
        stake_subsidy_decrease_rate: option::none(),
    }
}

public fun balance(mut self: Builder, value: Balance<SUI>): Builder {
    self.balance.fill(value);
    self
}
