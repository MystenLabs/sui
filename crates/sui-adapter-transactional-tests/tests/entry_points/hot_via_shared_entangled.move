// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

// Test of hot value rules for entry functions that involves chains of entangled result, where the
// heat is caused by taking a shared object by-value

//# init --addresses test=0x0 --accounts A

//# publish
module test::m;

public struct A has key, store { id: UID }
public struct B has key, store { id: UID }
public struct C has key, store { id: UID }
public struct D has key, store { id: UID }

public struct Shared has key { id: UID }

public fun init_shared(ctx: &mut TxContext) {
    sui::transfer::share_object(Shared { id: object::new(ctx) })
}

public fun a(ctx: &mut TxContext): A {
    A { id: object::new(ctx) }
}

public fun b(ctx: &mut TxContext): B {
    B { id: object::new(ctx) }
}

public fun c(ctx: &mut TxContext): C {
    C { id: object::new(ctx) }
}

public fun d(ctx: &mut TxContext): D {
    D { id: object::new(ctx) }
}

public fun entangle<T1, T2>(_: &T1, _: &T2) {
}

public fun heat<T>(_: &T, shared: Shared) {
    sui::transfer::share_object(shared)
}

entry fun play<T: key>(_: &T) {
}

public fun close(a: A, b: B, c: C, d: D) {
    let A { id: id_a } = a;
    let B { id: id_b } = b;
    let C { id: id_c } = c;
    let D { id: id_d } = d;
    object::delete(id_a);
    object::delete(id_b);
    object::delete(id_c);
    object::delete(id_d);
}

public fun close_(b: B, c: C, d: D) {
    let B { id: id_b } = b;
    let C { id: id_c } = c;
    let D { id: id_d } = d;
    object::delete(id_b);
    object::delete(id_c);
    object::delete(id_d);
}

public fun close_a(a: A) {
    let A { id: id_a } = a;
    object::delete(id_a);
}

//# run test::m::init_shared

//# programmable --sender A --inputs @A object(2,0)
// Cannot use D in an entry since it is hot
//> 0: test::m::a();
//> 1: test::m::b();
//> 2: test::m::c();
//> 3: test::m::d();
//> 4: test::m::entangle<test::m::A, test::m::B>(Result(0), Result(1));
//> 5: test::m::entangle<test::m::B, test::m::C>(Result(1), Result(2));
//> 6: test::m::entangle<test::m::C, test::m::D>(Result(2), Result(3));
//> 7: test::m::heat<test::m::D>(Result(3), Input(1));
//> test::m::play<test::m::D>(Result(3));
//> test::m::close(Result(0), Result(1), Result(2), Result(3))

//# programmable --sender A --inputs @A object(2,0)
// Cannot use C in an entry since it is hot via D
//> 0: test::m::a();
//> 1: test::m::b();
//> 2: test::m::c();
//> 3: test::m::d();
//> 4: test::m::entangle<test::m::A, test::m::B>(Result(0), Result(1));
//> 5: test::m::entangle<test::m::B, test::m::C>(Result(1), Result(2));
//> 6: test::m::entangle<test::m::C, test::m::D>(Result(2), Result(3));
//> 7: test::m::heat<test::m::D>(Result(3), Input(1));
//> test::m::play<test::m::C>(Result(2));
//> test::m::close(Result(0), Result(1), Result(2), Result(3));

//# programmable --sender A --inputs @A object(2,0)
// Cannot use B in an entry since it is hot via D
//> 0: test::m::a();
//> 1: test::m::b();
//> 2: test::m::c();
//> 3: test::m::d();
//> 4: test::m::entangle<test::m::A, test::m::B>(Result(0), Result(1));
//> 5: test::m::entangle<test::m::B, test::m::C>(Result(1), Result(2));
//> 6: test::m::entangle<test::m::C, test::m::D>(Result(2), Result(3));
//> 7: test::m::heat<test::m::D>(Result(3), Input(1));
//> test::m::play<test::m::B>(Result(1));
//> test::m::close(Result(0), Result(1), Result(2), Result(3));

//# programmable --sender A --inputs @A object(2,0)
// Cannot use A in an entry since it is hot via D
//> 0: test::m::a();
//> 1: test::m::b();
//> 2: test::m::c();
//> 3: test::m::d();
//> 4: test::m::entangle<test::m::A, test::m::B>(Result(0), Result(1));
//> 5: test::m::entangle<test::m::B, test::m::C>(Result(1), Result(2));
//> 6: test::m::entangle<test::m::C, test::m::D>(Result(2), Result(3));
//> 7: test::m::heat<test::m::D>(Result(3), Input(1));
//> test::m::play<test::m::A>(Result(0));
//> test::m::close(Result(0), Result(1), Result(2), Result(3));

//# programmable --sender A --inputs @A object(2,0)
// Cannot use A, even if the others are closed first, since it is always hot due to D's heat via
// shared objects by-value
//> 0: test::m::a();
//> 1: test::m::b();
//> 2: test::m::c();
//> 3: test::m::d();
//> 4: test::m::entangle<test::m::A, test::m::B>(Result(0), Result(1));
//> 5: test::m::entangle<test::m::B, test::m::C>(Result(1), Result(2));
//> 6: test::m::entangle<test::m::C, test::m::D>(Result(2), Result(3));
//> 7: test::m::heat<test::m::D>(Result(3), Input(1));
//> test::m::close_(Result(1), Result(2), Result(3));
//> test::m::play<test::m::A>(Result(0));
//> test::m::close_a(Result(0));
