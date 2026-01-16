// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

// tests multiple &TxContext parameters

//# init --addresses test=0x0

//# publish
module test::m;

public fun t2(_: &TxContext, _: &TxContext) {
}

public fun t3(_: &TxContext, _: &TxContext, _: &TxContext) {
}

public fun t4(_: &TxContext, _: &TxContext, _: &TxContext, _: &TxContext) {
}

public fun t3_u1(_: &TxContext, _: u64, _: &TxContext, _: &TxContext) {
}

public fun t3_u2(_: &TxContext, _: u64, _: &TxContext, _: &TxContext, _: u64) {
}

//# programmable
//> test::m::t2();

//# programmable
//> test::m::t3();

//# programmable
//> test::m::t4();

//# programmable --inputs 0
//> test::m::t3_u1(Input(0));

//# programmable --inputs 0 0
//> test::m::t3_u2(Input(0), Input(1));
