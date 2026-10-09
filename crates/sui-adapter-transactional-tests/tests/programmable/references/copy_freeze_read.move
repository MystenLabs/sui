// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

// Non-moving uses of a reference result: a Copy for every use but the last, a Freeze when a
// `&mut` is passed as `&`, and a Read when it is passed by value. Copies of a borrowed value stay
// copies. A Read releases the reference where it appears in the argument list.

//# init --addresses test=0x0 --accounts A --enable-feature-flags allow_references_in_ptbs

//# publish
module test::m;

use sui::coin::Coin;
use sui::sui::SUI;

public struct Obj has key, store { id: UID, inner: Inner }
public struct Inner has store, copy, drop { f: u64, g: u64 }
public struct NoCopy has drop { value: u64 }
public struct NoDrop has copy { f: u64 }

public fun new(ctx: &mut TxContext): Obj {
    Obj { id: object::new(ctx), inner: Inner { f: 0, g: 0 } }
}
public fun inner_val(): Inner { Inner { f: 0, g: 0 } }
public fun inner(o: &Obj): &Inner { &o.inner }
public fun inner_mut(o: &mut Obj): &mut Inner { &mut o.inner }
public fun f_mut(i: &mut Inner): &mut u64 { &mut i.f }
public fun copy_inner(i: &Inner): Inner { *i }
public fun id_mut<T>(t: &mut T): &mut T { t }
public fun use_imm<T>(_: &T) {}
public fun use_mut<T>(_: &mut T) {}
public fun write(r: &mut u64, v: u64) { *r = v }
public fun check(x: u64, v: u64) { assert!(x == v, 0) }
public fun check_inner(i: &Inner, f: u64) { assert!(i.f == f, 0) }
public fun take_inner(i: Inner, f: u64) { assert!(i.f == f, 0) }
public fun take(_: u64) {}
public fun copy_mut(_: u64, _: &mut u64) {}
public fun mut_copy(_: &mut u64, _: u64) {}
public fun value_at_least(c: &Coin<SUI>, v: u64) { assert!(c.value() >= v, 0) }
public fun no_copy(value: u64): NoCopy { NoCopy { value } }
public fun value_ref(n: &NoCopy): &u64 { &n.value }
public fun take_no_copy(_: NoCopy) {}
public fun no_drop(): NoDrop { NoDrop { f: 0 } }
public fun no_drop_f_mut(x: &mut NoDrop): &mut u64 { &mut x.f }
public fun take_no_drop(x: NoDrop) { let NoDrop { f: _ } = x; }
public fun take_no_drop_check(x: NoDrop, expected: u64) {
    let NoDrop { f } = x;
    assert!(f == expected, 0)
}
public fun delete(o: Obj) { let Obj { id, inner: _ } = o; object::delete(id) }

// --- copy and freeze of reference results ---

//# programmable
// VALID: a `&mut` result copied, frozen, copied again, and moved on its last use
//> 0: test::m::new();
//> 1: test::m::inner_mut(Result(0));
//> 2: test::m::use_mut<test::m::Inner>(Result(1));
//> 3: test::m::use_imm<test::m::Inner>(Result(1));
//> 4: test::m::use_mut<test::m::Inner>(Result(1));
//> 5: test::m::use_imm<test::m::Inner>(Result(1));
//> 6: test::m::delete(Result(0));

//# programmable
// VALID: an immutable result copied and moved
//> 0: test::m::new();
//> 1: test::m::inner(Result(0));
//> 2: test::m::use_imm<test::m::Inner>(Result(1));
//> 3: test::m::use_imm<test::m::Inner>(Result(1));
//> 4: test::m::delete(Result(0));

//# programmable --inputs 5
// VALID: a frozen use of the `&mut` result does not end its mutability
//> 0: test::m::new();
//> 1: test::m::inner_mut(Result(0));
//> 2: test::m::use_imm<test::m::Inner>(Result(1));
//> 3: test::m::f_mut(Result(1));
//> 4: test::m::write(Result(3), Input(0));
//> 5: test::m::delete(Result(0));

//# programmable --sender A --inputs 1000
// VALID: `&Coin<SUI>` from a `&mut` gas reference
//> 0: test::m::id_mut<sui::coin::Coin<sui::sui::SUI>>(Gas);
//> 1: test::m::value_at_least(Result(0), Input(0));

//# programmable --inputs 7 0
// VALID: freezing the parent permits a read while its mutable child is live; the copy keeps the old value
//> 0: test::m::new();
//> 1: test::m::inner_mut(Result(0));
//> 2: test::m::f_mut(Result(1));
//> 3: test::m::copy_inner(Result(1));
//> 4: test::m::write(Result(2), Input(0));
//> 5: test::m::check_inner(Result(1), Input(0));
//> 6: test::m::check_inner(Result(3), Input(1));
//> 7: test::m::delete(Result(0));

// --- copies of a borrowed value ---

//# programmable --inputs 0 7
// VALID: copy first, then `&mut`, in one call
//> 0: test::m::copy_mut(Input(0), Input(0));

//# programmable --inputs 0 7
// VALID: `&mut` first, then copy, in one call
//> 0: test::m::mut_copy(Input(0), Input(0));

//# programmable --inputs 0 7
// VALID: copy of a pure input while a `&mut` result into it is live, a later use sees the write
//> 0: test::m::id_mut<u64>(Input(0));
//> 1: test::m::take(Input(0));
//> 2: test::m::write(Result(0), Input(1));
//> 3: test::m::check(Input(0), Input(1));

//# programmable --inputs 7
// VALID: the last copy of a borrowed value stays a copy, the reference is still usable after
//> 0: test::m::id_mut<u64>(Input(0));
//> 1: test::m::take(Input(0));
//> 2: test::m::write(Result(0), Input(0));

//# programmable --inputs 7 0
// VALID: copy of a struct result while a reference into its field is live; the copy has the old value, the original the write
//> 0: test::m::inner_val();
//> 1: test::m::f_mut(Result(0));
//> 2: test::m::take_inner(Result(0), Input(1));
//> 3: test::m::write(Result(1), Input(0));
//> 4: test::m::check_inner(Result(0), Input(0));

//# programmable --inputs 7
// VALID: the last by-value use of a borrowed copy-only value stays a copy, so the original is still there to consume
//> 0: test::m::no_drop();
//> 1: test::m::no_drop_f_mut(Result(0));
//> 2: test::m::take_no_drop(Result(0));
//> 3: test::m::write(Result(1), Input(0));
//> 4: test::m::take_no_drop_check(Result(0), Input(0));

// --- reads ---

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
//> 3: test::m::take_no_copy(Result(0));

//# programmable --inputs 42
// VALID: a by-reference use before the read leaves the reference in place for the read
//> 0: test::m::id_mut<u64>(Input(0));
//> 1: test::m::use_mut<u64>(Result(0));
//> 2: test::m::check(Result(0), Input(0));

//# programmable --inputs 7 0
// VALID: a by-value read of the parent while its mutable child is live, which the pre-regex bytecode verifier rejects as READREF_EXISTS_MUTABLE_BORROW_ERROR
//> 0: test::m::inner_val();
//> 1: test::m::id_mut<test::m::Inner>(Result(0));
//> 2: test::m::f_mut(Result(1));
//> 3: test::m::take_inner(Result(1), Input(1));
//> 4: test::m::write(Result(2), Input(0));
//> 5: test::m::check_inner(Result(1), Input(0));
