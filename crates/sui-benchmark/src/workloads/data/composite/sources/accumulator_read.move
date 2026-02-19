// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

module basics::accumulator_read;

use sui::accumulator::AccumulatorRoot;
use sui::balance;
use sui::event;
use sui::sui::SUI;

public struct SettledBalanceEvent has copy, drop {
    value: u64,
}

entry fun read_settled_balance(root: &AccumulatorRoot, addr: address) {
    let value = balance::settled_funds_value<SUI>(root, addr);
    event::emit(SettledBalanceEvent { value });
}
