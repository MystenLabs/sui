// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

module test::test_module;

use oracle::data::{Self, Data};
use oracle::decimal_value::{Self, DecimalValue};
use oracle::meta_oracle;
use oracle::simple_oracle::{Self, SimpleOracle};
use std::option::{Self, Option};
use std::string;
use sui::object::{Self, UID};
use sui::transfer;
use sui::tx_context::{Self, TxContext};

public struct MockUSD has key, store {
    id: UID,
    amount: u64,
    decimals: u8,
}

public fun simple_fx_ptb(
    single_data: Option<Data<DecimalValue>>,
    mist_amount: u64,
    ctx: &mut TxContext,
) {
    let single_data = option::destroy_some(single_data);
    let value = data::value(&single_data);
    let decimals = decimal_value::decimal(value);
    let value = decimal_value::value(value);

    let amount = mist_amount * value;
    let usd = MockUSD {
        id: object::new(ctx),
        amount,
        decimals,
    };
    transfer::transfer(usd, tx_context::sender(ctx));
}

public fun simple_fx(oracle: &SimpleOracle, mist_amount: u64, ctx: &mut TxContext) {
    let single_data = simple_oracle::get_latest_data<DecimalValue>(oracle, string::utf8(b"SUIUSD"));
    let single_data = option::destroy_some(single_data);
    let value = data::value(&single_data);
    let decimals = decimal_value::decimal(value);
    let value = decimal_value::value(value);

    let amount = mist_amount * value;
    let usd = MockUSD {
        id: object::new(ctx),
        amount,
        decimals,
    };
    transfer::transfer(usd, tx_context::sender(ctx));
}

public fun trusted_fx(
    oracle1: &SimpleOracle,
    oracle2: &SimpleOracle,
    oracle3: &SimpleOracle,
    mist_amount: u64,
    ctx: &mut TxContext,
) {
    let mut meta_oracle = meta_oracle::new<DecimalValue>(3, 60000, string::utf8(b"SUIUSD"));
    meta_oracle::add_simple_oracle(&mut meta_oracle, oracle1);
    meta_oracle::add_simple_oracle(&mut meta_oracle, oracle2);
    meta_oracle::add_simple_oracle(&mut meta_oracle, oracle3);

    let trusted_data = meta_oracle::median(meta_oracle);
    let value = meta_oracle::value(&trusted_data);
    let decimals = decimal_value::decimal(value);
    let value = decimal_value::value(value);

    let amount = mist_amount * value;
    let usd = MockUSD {
        id: object::new(ctx),
        amount,
        decimals,
    };
    transfer::transfer(usd, tx_context::sender(ctx));
}
