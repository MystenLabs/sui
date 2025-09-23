// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

//# init --addresses Test=0x0 A=0x42

//# publish
module Test::M1 {
    use sui::coin::Coin;
    use sui::funds_accumulator;
    use sui::sui::SUI;

    public struct Object has key, store {
        id: UID,
        value: u64,
    }

    fun foo<T: key, T2: drop>(_p1: u64, value1: T, _value2: &Coin<T2>, _p2: u64): T {
        value1
    }

    public entry fun create(value: u64, recipient: address, ctx: &mut TxContext) {
        transfer::public_transfer(
            Object { id: object::new(ctx), value },
            recipient
        )
    }

    public entry fun check_withdrawal(
        withdrawal: funds_accumulator::Withdrawal<SUI>,
        _ctx: &mut TxContext,
    ) {
        assert!(
            funds_accumulator::withdrawal_limit(&withdrawal) == 0u256,
            0
        );
    }
}

//# run Test::M1::create --args 0 @A

//# run Test::M1::check_withdrawal --args withdraw(0,0x2::sui::SUI) --sender A

//# view-object 2,0
