// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

//# init --addresses test=0x0 --accounts A --enable-feature-flags allow_references_in_ptbs

//# publish
module test::m;

use sui::coin::Coin;
use sui::sui::SUI;

public struct NoCopy has drop { value: u64 }

public fun no_copy(value: u64): NoCopy { NoCopy { value } }
public fun value_ref(n: &NoCopy): &u64 { &n.value }
public fun id_imm<T>(t: &T): &T { t }
public fun id_mut<T>(t: &mut T): &mut T { t }
public fun use_imm<T>(_: &T) {}
public fun use_mut<T>(_: &mut T) {}
public fun write(r: &mut u64, v: u64) { *r = v }
public fun take(_: NoCopy) {}
public fun check(x: u64, v: u64) { assert!(x == v, 0) }
public fun amount_ref(_: &Coin<SUI>, x: &u64): &u64 { x }

//# programmable --inputs 42 7
// VALID: the root is borrowed mutably again after a read that was the reference's last use
//> 0: test::m::id_mut<u64>(Input(0));
//> 1: test::m::check(Result(0), Input(0));
//> 2: test::m::write(Input(0), Input(1));
//> 3: test::m::check(Input(0), Input(1));

//# programmable --inputs 1
// VALID: the referent is consumed after a read that was the reference's last use
//> 0: test::m::no_copy(Input(0));
//> 1: test::m::value_ref(Result(0));
//> 2: test::m::check(Result(1), Input(0));
//> 3: test::m::take(Result(0));

//# programmable --inputs 42
// VALID: a by-reference use before the read leaves the reference in place for the read
//> 0: test::m::id_mut<u64>(Input(0));
//> 1: test::m::use_mut<u64>(Result(0));
//> 2: test::m::check(Result(0), Input(0));

//# programmable --sender A --inputs 100 @A
// VALID: an amount read through a reference rooted in the split coin is released before the coin is written
//> 0: test::m::amount_ref(Gas, Input(0));
//> 1: SplitCoins(Gas, [Result(0)]);
//> 2: TransferObjects([Result(1)], Input(1));

//# programmable --sender A --inputs 100 @A
// INVALID: CannotWriteToExtendedReference at arg 0 of command 1, the coin-rooted amount reference is used again after the split
//> 0: test::m::amount_ref(Gas, Input(0));
//> 1: SplitCoins(Gas, [Result(0)]);
//> 2: TransferObjects([Result(1)], Input(1));
//> 3: test::m::use_imm<u64>(Result(0));
