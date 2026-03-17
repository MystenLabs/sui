// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

// tests error after serializing a large enum return value

//# init --addresses test=0x0 --accounts A

//# publish

module test::m;

use sui::event;

public enum X1 has copy, drop, store  {
    Big1(u8, u8, u8, u8, u8, u8, u8, u8, u8, u8, u8, u8, u8, u8, u8, u8),
}

public enum X2 has copy, drop, store {
    V1(X1, X1, X1),
    V2(X1, X1, X1),
    V3(X1, X1, X1),
}

public enum X3 has copy, drop, store {
    X2(X2, X2, X2),
    U64(u64),
}

public enum X4 has copy, drop, store {
    X2(X3, X3, X3),
    U64(u64),
}

public struct S has key, store {
    id: UID,
    inner: X4,
}

entry fun x1(): X1 {
    X1::Big1(0,0,0,0,0,0,0,0,0,0,0,0,0,0,0,0)
}

entry fun x3(): X3 {
    X3::U64(0)
}

entry fun x4(): X4 {
    X4::U64(0)
}

public fun e_x4() {
    event::emit(X4::U64(0))
}

public fun s_x4(ctx: &mut TxContext) {
    transfer::public_transfer(S { id: object::new(ctx), inner: X4::U64(0) }, ctx.sender());
}

//# programmable --sender A
//> test::m::x1()

//# programmable --sender A
//> test::m::x3()

//# programmable --sender A
//> test::m::x4()

//# programmable --sender A
//> test::m::e_x4()

//# programmable --sender A
//> test::m::s_x4()
