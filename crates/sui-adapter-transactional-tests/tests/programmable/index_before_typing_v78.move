// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

// in this protocol version, these typing errors should happen before bounds checking

//# init --addresses test=0x0 --accounts A --protocol-version 76

//# publish
module test::m;

public struct A() has copy, drop;
public struct B() has copy, drop;

public fun a(): A { A() }
public fun a2(): (A, A) { (A(), A()) }
public fun take_b(_: B, _: B) {}

// input out of bounds at len
//# programmable --inputs 0u8
//> test::m::take_b(Input(0), Input(1))

// input out of bounds
//# programmable --inputs 0u8
//> test::m::take_b(Input(0), Input(100))

// result out of bounds at len
//# programmable
//> test::m::a();
//> test::m::take_b(Result(0), Result(1))

// result out of bounds
//# programmable
//> test::m::a();
//> test::m::take_b(Result(0), Result(5123))

// nested results out of bounds at len
//# programmable
//> test::m::a();
//> test::m::a2();
//> test::m::take_b(Result(0), NestedResult(2, 0))

// nested results out of bounds
//# programmable
//> test::m::a();
//> test::m::a2();
//> test::m::take_b(Result(0), NestedResult(115, 0))

// nested secondary out of bounds barely
//# programmable
//> test::m::a();
//> test::m::a2();
//> test::m::take_b(Result(0), NestedResult(1, 2))

// nested secondary out of bounds
//# programmable
//> test::m::a();
//> test::m::a2();
//> test::m::take_b(Result(0), NestedResult(1, 2104))
