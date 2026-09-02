// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

// tests TxContext taken by value in Move call parameters

//# init --addresses test=0x0 --accounts A

//# publish
module test::m;

public fun by_value(_: TxContext) {
    abort 0
}

public fun by_value_trailing(_: u64, _: TxContext) {
    abort 0
}

public fun gen<T>(_: T) {
    abort 0
}

//# programmable --inputs 0
// a pure argument cannot be used for TxContext, checked before the signature rules
//> test::m::by_value(Input(0));

//# programmable --sender A --inputs 0 --dev-inspect
// TxContext cannot be taken by value, even in dev-inspect where arbitrary pure values are allowed
//> test::m::by_value(Input(0));

//# programmable --sender A --inputs 0 0 --dev-inspect
// TxContext cannot be taken by value, at any position
//> test::m::by_value_trailing(Input(0), Input(1));

//# programmable --sender A --inputs 0 --dev-inspect
// TxContext cannot be taken by value, even via generic instantiation
//> test::m::gen<sui::tx_context::TxContext>(Input(0));
