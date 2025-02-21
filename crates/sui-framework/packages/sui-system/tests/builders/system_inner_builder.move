// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

module sui_system::system_inner_builder;

use sui::balance::{Self, Balance};
use sui::sui::SUI;
use sui_system::stake_subsidy::StakeSubsidy;
use sui_system::sui_system_state_inner::{
    Self,
    SuiSystemStateInner,
    SuiSystemStateInnerV2,
    SystemParameters
};
use sui_system::system_params_builder;
use sui_system::validator::Validator;

public struct Builder {
    validators: vector<Validator>,
    initial_storage_fund: Option<Balance<SUI>>,
    protocol_version: Option<u64>,
    epoch_start_timestamp_ms: Option<u64>,
    parameters: Option<SystemParameters>,
    stake_subsidy: Option<StakeSubsidy>,
}

public fun validators(mut builder: Builder, validators: vector<Validator>): Builder {
    builder.validators.append(validators);
    builder
}

public fun initial_storage_fund(mut builder: Builder, initial_storage_fund: Balance<SUI>): Builder {
    builder.initial_storage_fund.fill(initial_storage_fund);
    builder
}

public fun protocol_version(mut builder: Builder, protocol_version: u64): Builder {
    builder.protocol_version = option::some(protocol_version);
    builder
}

public fun epoch_start_timestamp_ms(mut builder: Builder, epoch_start_timestamp_ms: u64): Builder {
    builder.epoch_start_timestamp_ms = option::some(epoch_start_timestamp_ms);
    builder
}

public fun parameters(mut builder: Builder, parameters: SystemParameters): Builder {
    builder.parameters.fill(parameters);
    builder
}

public fun build(self: Builder, subsidy: StakeSubsidy, ctx: &mut TxContext): SuiSystemStateInner {
    let Builder {
        validators,
        initial_storage_fund,
        protocol_version,
        epoch_start_timestamp_ms,
        parameters,
        stake_subsidy,
    } = self;

    sui_system_state_inner::create(
        validators,
        initial_storage_fund.destroy_or!(balance::zero()),
        protocol_version.destroy_or!(0),
        epoch_start_timestamp_ms.destroy_or!(0),
        parameters.destroy_or!(system_params_builder::new().build(ctx)),
        stake_subsidy.destroy_or!(stake_subsidy_builder::new().build(ctx)),
        ctx,
    )
}

public fun build_v2(
    self: Builder,
    subsidy: StakeSubsidy,
    ctx: &mut TxContext,
): SuiSystemStateInnerV2 {
    self.build(subsidy, ctx).v1_to_v2()
}
