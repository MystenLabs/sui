// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

//# init --simulator --accounts A --addresses test=0x0 --reference-gas-price 700 

//# publish
module test::gas_test;

public fun gas_checks(price: u64, delta: u64, ctx: &TxContext) {
    let rgp = ctx.reference_gas_price();
    let gas_price = ctx.gas_price();
    assert!(rgp == 700, 100);
    assert!(gas_price == price , 101);
    assert!(gas_price >= rgp + delta, 102);
}

//# programmable --sender A --gas-price 800 --inputs 800 100
// success, gas price(800) higher than reference gas price(700) + 100
//> test::gas_test::gas_checks(Input(0), Input(1))

//# programmable --sender A --gas-price 2000 --inputs 2000 100
// success, gas price(2000) higher than reference gas price(700) + 100
//> test::gas_test::gas_checks(Input(0), Input(1))

//# programmable --sender A --gas-price 800 --inputs 800 300
// failure, gas price(800) lower than reference gas price(700) + 300
//> test::gas_test::gas_checks(Input(0), Input(1))

