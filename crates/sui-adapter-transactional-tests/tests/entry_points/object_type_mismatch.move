// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

// Tests passing objects of the wrong type to entry functions

//# init --addresses test=0x0 --accounts A

//# publish
module test::m;

public struct ObjA has key, store {
    id: UID,
}

public struct ObjB has key, store {
    id: UID,
}

public struct Wrapper<phantom T> has key, store {
    id: UID,
}

public entry fun mint_a(ctx: &mut TxContext) {
    transfer::public_transfer(ObjA { id: object::new(ctx) }, ctx.sender())
}

public entry fun mint_b(ctx: &mut TxContext) {
    transfer::public_transfer(ObjB { id: object::new(ctx) }, ctx.sender())
}

public entry fun mint_wrapper_u64(ctx: &mut TxContext) {
    transfer::public_transfer(Wrapper<u64> { id: object::new(ctx) }, ctx.sender())
}

public entry fun mint_wrapper_bool(ctx: &mut TxContext) {
    transfer::public_transfer(Wrapper<bool> { id: object::new(ctx) }, ctx.sender())
}

public entry fun take_a(_o: &ObjA) {}
public entry fun take_b(_o: &ObjB) {}
public entry fun take_wrapper_u64(_o: &Wrapper<u64>) {}
public entry fun take_wrapper_bool(_o: &Wrapper<bool>) {}
public entry fun take_generic<T: key + store>(_o: &T) {}

public entry fun take_a_mut(_o: &mut ObjA) {}
public entry fun take_wrapper_u64_mut(_o: &mut Wrapper<u64>) {}
public entry fun take_wrapper_bool_mut(_o: &mut Wrapper<bool>) {}

public entry fun destroy_a(o: ObjA) { let ObjA { id } = o; object::delete(id) }
public entry fun destroy_b(o: ObjB) { let ObjB { id } = o; object::delete(id) }
public entry fun destroy_wrapper_u64(o: Wrapper<u64>) { let Wrapper { id } = o; object::delete(id) }
public entry fun destroy_wrapper_bool(o: Wrapper<bool>) { let Wrapper { id } = o; object::delete(id) }

// mint one of each
// object(2,0) = ObjA
//# run test::m::mint_a --sender A

// object(3,0) = ObjB
//# run test::m::mint_b --sender A

// object(4,0) = Wrapper<u64>
//# run test::m::mint_wrapper_u64 --sender A

// object(5,0) = Wrapper<bool>
//# run test::m::mint_wrapper_bool --sender A

// pass ObjB where &ObjA expected
//# run test::m::take_a --sender A --args object(3,0)

// pass ObjA where &ObjB expected
//# run test::m::take_b --sender A --args object(2,0)

// pass Wrapper<bool> where &Wrapper<u64> expected
//# run test::m::take_wrapper_u64 --sender A --args object(5,0)

// pass Wrapper<u64> where &Wrapper<bool> expected
//# run test::m::take_wrapper_bool --sender A --args object(4,0)

// pass ObjA where &T expected with T=ObjB
//# run test::m::take_generic --type-args test::m::ObjB --sender A --args object(2,0)

// &mut ref mismatches: pass ObjB where &mut ObjA expected
//# run test::m::take_a_mut --sender A --args object(3,0)

// pass Wrapper<bool> where &mut Wrapper<u64> expected
//# run test::m::take_wrapper_u64_mut --sender A --args object(5,0)

// pass Wrapper<u64> where &mut Wrapper<bool> expected
//# run test::m::take_wrapper_bool_mut --sender A --args object(4,0)

// happy paths (immutable ref)
//# run test::m::take_a --sender A --args object(2,0)

//# run test::m::take_wrapper_u64 --sender A --args object(4,0)

//# run test::m::take_generic --type-args test::m::ObjA --sender A --args object(2,0)

// happy paths (mutable ref)
//# run test::m::take_a_mut --sender A --args object(2,0)

//# run test::m::take_wrapper_u64_mut --sender A --args object(4,0)

// by-value mismatches (these consume the object, so test mismatches first)
// pass ObjB where ObjA expected
//# run test::m::destroy_a --sender A --args object(3,0)

// pass Wrapper<bool> where Wrapper<u64> expected
//# run test::m::destroy_wrapper_u64 --sender A --args object(5,0)

// pass Wrapper<u64> where Wrapper<bool> expected
//# run test::m::destroy_wrapper_bool --sender A --args object(4,0)

// happy paths (by value, consume the objects last)
//# run test::m::destroy_a --sender A --args object(2,0)

//# run test::m::destroy_wrapper_u64 --sender A --args object(4,0)

//# run test::m::destroy_wrapper_bool --sender A --args object(5,0)

//# run test::m::destroy_b --sender A --args object(3,0)
