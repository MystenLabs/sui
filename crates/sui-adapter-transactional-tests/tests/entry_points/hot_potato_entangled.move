// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

// Test of hot value rules for entry functions that involves chains of entangled results

//# init --addresses test=0x0 --accounts A

//# publish
module test::m;

public struct A has key, store { id: UID }
public struct B has key, store { id: UID }
public struct C has key, store { id: UID }
public struct D has key, store { id: UID }

public struct Hot {}

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

public fun heat<T>(_: &T): Hot {
    Hot {}
}

public fun cool(x: Hot) {
    let Hot {} = x;
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

//# programmable --sender A --inputs @A
// Cannot use D in an entry since it is hot
//> 0: test::m::a();
//> 1: test::m::b();
//> 2: test::m::c();
//> 3: test::m::d();
//> 4: test::m::entangle<test::m::A, test::m::B>(Result(0), Result(1));
//> 5: test::m::entangle<test::m::B, test::m::C>(Result(1), Result(2));
//> 6: test::m::entangle<test::m::C, test::m::D>(Result(2), Result(3));
//> 7: test::m::heat<test::m::D>(Result(3));
//> test::m::play<test::m::D>(Result(3));
//> test::m::cool(Result(7));
//> test::m::close(Result(0), Result(1), Result(2), Result(3));

//# programmable --sender A --inputs @A
// Cannot use C in an entry since it is hot via D
//> 0: test::m::a();
//> 1: test::m::b();
//> 2: test::m::c();
//> 3: test::m::d();
//> 4: test::m::entangle<test::m::A, test::m::B>(Result(0), Result(1));
//> 5: test::m::entangle<test::m::B, test::m::C>(Result(1), Result(2));
//> 6: test::m::entangle<test::m::C, test::m::D>(Result(2), Result(3));
//> 7: test::m::heat<test::m::D>(Result(3));
//> test::m::play<test::m::C>(Result(2));
//> test::m::cool(Result(7));
//> test::m::close(Result(0), Result(1), Result(2), Result(3));

//# programmable --sender A --inputs @A
// Cannot use B in an entry since it is hot via D via C
//> 0: test::m::a();
//> 1: test::m::b();
//> 2: test::m::c();
//> 3: test::m::d();
//> 4: test::m::entangle<test::m::A, test::m::B>(Result(0), Result(1));
//> 5: test::m::entangle<test::m::B, test::m::C>(Result(1), Result(2));
//> 6: test::m::entangle<test::m::C, test::m::D>(Result(2), Result(3));
//> 7: test::m::heat<test::m::D>(Result(3));
//> test::m::play<test::m::B>(Result(1));
//> test::m::cool(Result(7));
//> test::m::close(Result(0), Result(1), Result(2), Result(3));

//# programmable --sender A --inputs @A
// Cannot use A in an entry since it is hot via D via C via B
//> 0: test::m::a();
//> 1: test::m::b();
//> 2: test::m::c();
//> 3: test::m::d();
//> 4: test::m::entangle<test::m::A, test::m::B>(Result(0), Result(1));
//> 5: test::m::entangle<test::m::B, test::m::C>(Result(1), Result(2));
//> 6: test::m::entangle<test::m::C, test::m::D>(Result(2), Result(3));
//> 7: test::m::heat<test::m::D>(Result(3));
//> test::m::play<test::m::A>(Result(0));
//> test::m::cool(Result(7));
//> test::m::close(Result(0), Result(1), Result(2), Result(3));

//# programmable --sender A --inputs @A
// All allowed (in v2) if Hot is cooled first
//> 0: test::m::a();
//> 1: test::m::b();
//> 2: test::m::c();
//> 3: test::m::d();
//> 4: test::m::entangle<test::m::A, test::m::B>(Result(0), Result(1));
//> 5: test::m::entangle<test::m::B, test::m::C>(Result(1), Result(2));
//> 6: test::m::entangle<test::m::C, test::m::D>(Result(2), Result(3));
//> 7: test::m::heat<test::m::D>(Result(3));
//> test::m::cool(Result(7));
//> test::m::play<test::m::A>(Result(0));
//> test::m::play<test::m::B>(Result(1));
//> test::m::play<test::m::C>(Result(2));
//> test::m::play<test::m::D>(Result(3));
//> test::m::close(Result(0), Result(1), Result(2), Result(3));
