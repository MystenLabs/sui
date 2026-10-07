// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

// Writes through references reach the underlying value: later reads in the same transaction see
// them, and they persist in objects. Runtime behavior only; every task here is accepted statically.

//# init --addresses test=0x0 p=0x0 q=0x0 q_2=0x0 --accounts A B --enable-feature-flags allow_references_in_ptbs

//# publish
module test::m;

public struct Obj has key, store { id: UID, inner: Inner }
public struct Inner has store, copy, drop { f: u64, g: u64 }

public fun new(ctx: &mut TxContext): Obj {
    Obj { id: object::new(ctx), inner: Inner { f: 0, g: 0 } }
}
public fun share(ctx: &mut TxContext) {
    transfer::public_share_object(Obj { id: object::new(ctx), inner: Inner { f: 0, g: 0 } })
}
public fun uid(o: &Obj): &UID { &o.id }
public fun uid_mut(o: &mut Obj): &mut UID { &mut o.id }
public fun inner(o: &Obj): &Inner { &o.inner }
public fun inner_mut(o: &mut Obj): &mut Inner { &mut o.inner }
public fun f(i: &Inner): &u64 { &i.f }
public fun f_mut(i: &mut Inner): &mut u64 { &mut i.f }
public fun f_g_mut(i: &mut Inner): (&mut u64, &mut u64) { (&mut i.f, &mut i.g) }
public fun set_inner(i: &mut Inner, f: u64, g: u64) { i.f = f; i.g = g }
public fun id_mut<T>(t: &mut T): &mut T { t }
public fun write(r: &mut u64, v: u64) { *r = v }
public fun write_u8(r: &mut u8, v: u8) { *r = v }
public fun check(r: &u64, v: u64) { assert!(*r == v, 0) }
public fun check_u8(r: &u8, v: u8) { assert!(*r == v, 0) }
public fun check_bool(r: &bool, v: bool) { assert!(*r == v, 0) }
public fun check_val(x: u64, v: u64) { assert!(x == v, 0) }
public fun check_two_vals(x: u64, y: u64, v: u64) { assert!(x == v && y == v, 0) }
public fun check_inner(i: &Inner, f: u64) { assert!(i.f == f, 0) }
public fun check_elem(v: &vector<u64>, i: u64, e: u64) { assert!(v[i] == e, 0) }
public fun check_len(v: &vector<u64>, n: u64) { assert!(v.length() == n, 0) }
public fun delete(o: Obj) { let Obj { id, inner: _ } = o; object::delete(id) }

//# publish --upgradeable --sender A
module q::m {
    public fun x(): u64 { 0 }
}

//# stage-package
module p::m {
    public struct Marker has key { id: UID }
    fun init(ctx: &mut TxContext) {
        transfer::transfer(Marker { id: object::new(ctx) }, ctx.sender())
    }
}

//# stage-package
module q_2::m {
    public fun x(): u64 { 1 }
}

//# programmable --sender A --inputs @A
//> 0: test::m::new();
//> 1: TransferObjects([Result(0)], Input(0));

//# programmable --sender A
//> 0: test::m::share();

//# programmable --sender A --inputs 1000 @A
//> 0: SplitCoins(Gas, [Input(0)]);
//> 1: TransferObjects([Result(0)], Input(1));

// --- reads in the same transaction see the write ---

//# programmable --inputs 5
// VALID: a frozen read of the parent after a write through the child
//> 0: test::m::new();
//> 1: test::m::inner_mut(Result(0));
//> 2: test::m::f_mut(Result(1));
//> 3: test::m::write(Result(2), Input(0));
//> 4: test::m::check_inner(Result(1), Input(0));
//> 5: test::m::delete(Result(0));

//# programmable --inputs 5
// VALID: an immutable child taken after the write sees it
//> 0: test::m::new();
//> 1: test::m::inner_mut(Result(0));
//> 2: test::m::f_mut(Result(1));
//> 3: test::m::write(Result(2), Input(0));
//> 4: test::m::f(Result(1));
//> 5: test::m::check(Result(4), Input(0));
//> 6: test::m::delete(Result(0));

//# programmable --inputs 5 6
// VALID: two writes through two successive references, the last wins
//> 0: test::m::new();
//> 1: test::m::inner_mut(Result(0));
//> 2: test::m::f_mut(Result(1));
//> 3: test::m::write(Result(2), Input(0));
//> 4: test::m::f_mut(Result(1));
//> 5: test::m::write(Result(4), Input(1));
//> 6: test::m::check_inner(Result(1), Input(1));
//> 7: test::m::delete(Result(0));

//# programmable --inputs 0 7
// VALID: a pure input written through `&mut`, then seen by reference and by value
//> 0: test::m::id_mut<u64>(Input(0));
//> 1: test::m::write(Result(0), Input(1));
//> 2: test::m::check(Input(0), Input(1));
//> 3: test::m::check_val(Input(0), Input(1));

//# programmable --inputs 0 7 8
// VALID: a second reference sees the first write and its own write is seen after
//> 0: test::m::id_mut<u64>(Input(0));
//> 1: test::m::write(Result(0), Input(1));
//> 2: test::m::id_mut<u64>(Input(0));
//> 3: test::m::check(Result(2), Input(1));
//> 4: test::m::write(Result(2), Input(2));
//> 5: test::m::check(Input(0), Input(2));

//# programmable --inputs 0 7
// VALID: two by-value uses of the input both see the write
//> 0: test::m::id_mut<u64>(Input(0));
//> 1: test::m::write(Result(0), Input(1));
//> 2: test::m::check_two_vals(Input(0), Input(0), Input(1));

//# programmable --inputs 0u8 7u8 false
// VALID: the bool view of a pure input is a different value from the u8 view
//> 0: test::m::id_mut<u8>(Input(0));
//> 1: test::m::write_u8(Result(0), Input(1));
//> 2: test::m::check_u8(Input(0), Input(1));
//> 3: test::m::check_bool(Input(0), Input(2));

//# programmable --inputs 1 2 0 1 9
// VALID: writes to two vector elements through separate references
//> 0: MakeMoveVec<u64>([Input(0), Input(1)]);
//> 1: std::vector::borrow_mut<u64>(Result(0), Input(2));
//> 2: test::m::write(Result(1), Input(4));
//> 3: std::vector::borrow_mut<u64>(Result(0), Input(3));
//> 4: test::m::write(Result(3), Input(4));
//> 5: test::m::check_elem(Result(0), Input(2), Input(4));
//> 6: test::m::check_elem(Result(0), Input(3), Input(4));

//# programmable --sender A --inputs object(5,0) 1 2 9
// VALID: a dynamic field added, written, read, and removed through references into the UID
//> 0: test::m::uid_mut(Input(0));
//> 1: sui::dynamic_field::add<u64, u64>(Result(0), Input(1), Input(2));
//> 2: sui::dynamic_field::borrow_mut<u64, u64>(Result(0), Input(1));
//> 3: test::m::write(Result(2), Input(3));
//> 4: test::m::uid(Input(0));
//> 5: sui::dynamic_field::borrow<u64, u64>(Result(4), Input(1));
//> 6: test::m::check(Result(5), Input(3));
//> 7: sui::dynamic_field::remove<u64, u64>(Result(0), Input(1));
//> 8: test::m::check_val(Result(7), Input(3));

// --- writes persist in objects ---

//# programmable --sender A --inputs object(5,0) 1 2
// VALID: both field writes through the returned struct reference persist in an owned object
//> 0: test::m::inner_mut(Input(0));
//> 1: test::m::set_inner(Result(0), Input(1), Input(2));

//# view-object 5,0

//# programmable --sender A --inputs object(5,0) 3 4
// VALID: writes through sibling field references persist
//> 0: test::m::inner_mut(Input(0));
//> 1: test::m::f_g_mut(Result(0));
//> 2: test::m::write(NestedResult(1,0), Input(1));
//> 3: test::m::write(NestedResult(1,1), Input(2));

//# view-object 5,0

//# programmable --sender A --inputs object(6,0) 7
// VALID: a write through a reference into a shared object persists
//> 0: test::m::inner_mut(Input(0));
//> 1: test::m::f_mut(Result(0));
//> 2: test::m::write(Result(1), Input(1));

//# view-object 6,0

//# programmable --sender A --inputs immshared(6,0) 7
// VALID: an immutable reference into a read-only shared object sees the persisted write
//> 0: test::m::inner(Input(0));
//> 1: test::m::f(Result(0));
//> 2: test::m::check(Result(1), Input(1));

//# programmable --sender A --inputs 42 @A
// VALID: a fresh object mutated through a reference is transferred with the mutation
//> 0: test::m::new();
//> 1: test::m::inner_mut(Result(0));
//> 2: test::m::f_mut(Result(1));
//> 3: test::m::write(Result(2), Input(0));
//> 4: TransferObjects([Result(0)], Input(1));

//# view-object 24,0

//# programmable --sender A --inputs object(7,0) 300 @B
// VALID: a split through a `&mut Balance` into a coin input is a new coin and a smaller input
//> 0: sui::coin::balance_mut<sui::sui::SUI>(Input(0));
//> 1: sui::balance::split<sui::sui::SUI>(Result(0), Input(1));
//> 2: sui::coin::from_balance<sui::sui::SUI>(Result(1));
//> 3: TransferObjects([Result(2)], Input(2));

//# view-object 7,0

//# view-object 26,0

// --- references live across Publish and Upgrade ---

//# programmable --sender A --inputs object(5,0) 7 @A
// VALID: Publish with an `init` between a reference's creation and its use
//> 0: test::m::inner_mut(Input(0));
//> 1: test::m::f_mut(Result(0));
//> 2: Publish(p, [sui, std]);
//> 3: test::m::write(Result(1), Input(1));
//> 4: TransferObjects([Result(2)], Input(2));

//# view-object 5,0

//# programmable --sender A --inputs object(2,1) 0u8 digest(q_2) object(5,0) 9
// VALID: Upgrade between a reference's creation and its use
//> 0: sui::package::authorize_upgrade(Input(0), Input(1), Input(2));
//> 1: test::m::inner_mut(Input(3));
//> 2: test::m::f_mut(Result(1));
//> 3: Upgrade(q_2, [sui, std], q, Result(0));
//> 4: test::m::write(Result(2), Input(4));
//> 5: sui::package::commit_upgrade(Input(0), Result(3));

//# view-object 5,0
