// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

// Moving a location while a reference into it is live is rejected with CannotMoveBorrowedValue,
// for every kind of location and every consuming command. Borrowing a location after it was moved
// is rejected with ArgumentWithoutValue. Both checks run per argument, in argument order.

//# init --addresses test=0x0 q=0x0 q_2=0x0 --accounts A B --enable-feature-flags allow_references_in_ptbs

//# publish
module test::m;

use sui::transfer::Receiving;

public struct Obj has key, store { id: UID, inner: Inner }
public struct Inner has store, copy, drop { f: u64, g: u64 }
public struct Tagged has key, store { id: UID, to: address }
public struct NoCopy has drop { f: u64 }
public struct Outer has drop { nc: NoCopy }
public struct Potato { v: u64 }
public struct Parent has key, store { id: UID }
public struct Child has key, store { id: UID }

public fun new(ctx: &mut TxContext): Obj {
    Obj { id: object::new(ctx), inner: Inner { f: 0, g: 0 } }
}
public fun share(ctx: &mut TxContext) {
    transfer::public_share_object(Obj { id: object::new(ctx), inner: Inner { f: 0, g: 0 } })
}
public fun new_parent(ctx: &mut TxContext): Parent { Parent { id: object::new(ctx) } }
public fun new_child(p: &Parent, ctx: &mut TxContext) {
    transfer::public_transfer(Child { id: object::new(ctx) }, object::id_address(p))
}
public fun new_tagged(to: address, ctx: &mut TxContext): Tagged {
    Tagged { id: object::new(ctx), to }
}
public fun to(t: &Tagged): &address { &t.to }
public fun inner(o: &Obj): &Inner { &o.inner }
public fun inner_mut(o: &mut Obj): &mut Inner { &mut o.inner }
public fun f_mut(i: &mut Inner): &mut u64 { &mut i.f }
public fun nc(): NoCopy { NoCopy { f: 0 } }
public fun outer(): Outer { Outer { nc: NoCopy { f: 0 } } }
public fun nc_mut(o: &mut Outer): &mut NoCopy { &mut o.nc }
public fun nc_f(n: &NoCopy): &u64 { &n.f }
public fun nc_f_mut(n: &mut NoCopy): &mut u64 { &mut n.f }
public fun take(_: NoCopy) {}
public fun take_outer(_: Outer) {}
public fun take_then_read(n: NoCopy, x: u64) { assert!(x == n.f, 0) }
public fun imm_and_take(_: &NoCopy, _: NoCopy) { abort 0 }
public fun mut_and_take(_: &mut NoCopy, _: NoCopy) { abort 0 }
public fun take_and_imm(_: NoCopy, _: &NoCopy) { abort 0 }
public fun take_and_mut(_: NoCopy, _: &mut NoCopy) { abort 0 }
public fun take_and_u64(_: NoCopy, _: &mut u64) { abort 0 }
public fun u64_and_take(_: &mut u64, _: NoCopy) { abort 0 }
public fun imm_u64_and_take(_: &u64, _: NoCopy) { abort 0 }
public fun take_then_mut(o: Obj, _: &mut Inner) { delete(o) }
public fun mut_then_take(_: &mut Inner, o: Obj) { delete(o) }
public fun open(o: &mut Obj): (Potato, &mut Inner) { (Potato { v: 1 }, &mut o.inner) }
public fun close_and_delete(p: Potato, o: Obj) { let Potato { v: _ } = p; delete(o) }
public fun recv(r: &Receiving<Child>): &Receiving<Child> { r }
public fun recv_mut(r: &mut Receiving<Child>): &mut Receiving<Child> { r }
public fun receive(p: &mut Parent, r: Receiving<Child>): Child {
    transfer::public_receive(&mut p.id, r)
}
public fun r_then_v(_: &Receiving<Child>, _: Receiving<Child>) { abort 0 }
public fun v_then_r(_: Receiving<Child>, _: &Receiving<Child>) { abort 0 }
public fun id<T>(t: &T): &T { t }
public fun id_mut<T>(t: &mut T): &mut T { t }
public fun use_imm<T>(_: &T) {}
public fun use_mut<T>(_: &mut T) {}
public fun write(r: &mut u64, v: u64) { *r = v }
public fun delete(o: Obj) { let Obj { id, inner: _ } = o; object::delete(id) }

//# publish --upgradeable --sender A
module q::m {
    public fun x(): u64 { 0 }
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

//# programmable --sender A --inputs @A
//> 0: test::m::new_parent();
//> 1: TransferObjects([Result(0)], Input(0));

//# programmable --sender A --inputs object(6,0)
//> 0: test::m::new_child(Input(0));

//# programmable --sender A --inputs 1000 @A
//> 0: SplitCoins(Gas, [Input(0)]);
//> 1: sui::coin::into_balance<sui::sui::SUI>(Result(0));
//> 2: sui::balance::send_funds<sui::sui::SUI>(Result(1), Input(1));

//# create-checkpoint

// --- a result moved while a reference into it is live ---

//# programmable --inputs 1
// INVALID: CannotMoveBorrowedValue at arg 0 of command 2, `&mut` child live
//> 0: test::m::nc();
//> 1: test::m::nc_f_mut(Result(0));
//> 2: test::m::take(Result(0));
//> 3: test::m::write(Result(1), Input(0));

//# programmable
// INVALID: CannotMoveBorrowedValue at arg 0 of command 2, `&` child live
//> 0: test::m::nc();
//> 1: test::m::nc_f(Result(0));
//> 2: test::m::take(Result(0));
//> 3: test::m::use_imm<u64>(Result(1));

//# programmable --inputs 1
// INVALID: CannotMoveBorrowedValue at arg 0 of command 3, grandchild live
//> 0: test::m::outer();
//> 1: test::m::nc_mut(Result(0));
//> 2: test::m::nc_f_mut(Result(1));
//> 3: test::m::take_outer(Result(0));
//> 4: test::m::write(Result(2), Input(0));

// --- a value and a reference into it in one call, in both orders ---

//# programmable
// INVALID: CannotMoveBorrowedValue at arg 1 of command 1, `&` then the value
//> 0: test::m::nc();
//> 1: test::m::imm_and_take(Result(0), Result(0));

//# programmable
// INVALID: CannotMoveBorrowedValue at arg 1 of command 1, `&mut` then the value
//> 0: test::m::nc();
//> 1: test::m::mut_and_take(Result(0), Result(0));

//# programmable
// INVALID: ArgumentWithoutValue at arg 1 of command 1, the value then a `&` of it
//> 0: test::m::nc();
//> 1: test::m::take_and_imm(Result(0), Result(0));

//# programmable
// INVALID: ArgumentWithoutValue at arg 1 of command 1, the value then a `&mut` of it
//> 0: test::m::nc();
//> 1: test::m::take_and_mut(Result(0), Result(0));

//# programmable
// INVALID: CannotMoveBorrowedValue at arg 0 of command 2, the value moved with its own field reference
//> 0: test::m::nc();
//> 1: test::m::nc_f_mut(Result(0));
//> 2: test::m::take_and_u64(Result(0), Result(1));

//# programmable
// INVALID: CannotMoveBorrowedValue at arg 1 of command 2, the field reference first
//> 0: test::m::nc();
//> 1: test::m::nc_f_mut(Result(0));
//> 2: test::m::u64_and_take(Result(1), Result(0));

//# programmable
// INVALID: CannotMoveBorrowedValue at arg 1 of command 2, a frozen `&mut` result lives through the call, unlike a read
//> 0: test::m::nc();
//> 1: test::m::nc_f_mut(Result(0));
//> 2: test::m::imm_u64_and_take(Result(1), Result(0));

//# programmable --inputs 1
// INVALID: CannotMoveBorrowedValue at arg 0 of command 2, the root is moved before a last-use read releases the reference
//> 0: test::m::nc();
//> 1: test::m::nc_f(Result(0));
//> 2: test::m::take_then_read(Result(0), Result(1));

//# programmable --sender A --inputs @A
// INVALID: CannotMoveBorrowedValue at arg 0 of command 2, TransferObjects processes objects before the recipient read
//> 0: test::m::new_tagged(Input(0));
//> 1: test::m::to(Result(0));
//> 2: TransferObjects([Result(0)], Result(1));

// --- object inputs ---

//# programmable --sender A --inputs object(4,0)
// INVALID: CannotMoveBorrowedValue at arg 0 of command 1, an owned input by value then its `&mut Inner`
//> 0: test::m::inner_mut(Input(0));
//> 1: test::m::take_then_mut(Input(0), Result(0));

//# programmable --sender A --inputs object(4,0)
// INVALID: CannotMoveBorrowedValue at arg 1 of command 1, `&mut Inner` then the owned input by value
//> 0: test::m::inner_mut(Input(0));
//> 1: test::m::mut_then_take(Result(0), Input(0));

//# programmable --sender A --inputs object(4,0) @B 1
// INVALID: CannotMoveBorrowedValue at arg 0 of command 2, an owned input transferred while borrowed
//> 0: test::m::inner_mut(Input(0));
//> 1: test::m::f_mut(Result(0));
//> 2: TransferObjects([Input(0)], Input(1));
//> 3: test::m::write(Result(1), Input(2));

//# programmable --sender A --inputs object(4,0)
// INVALID: CannotMoveBorrowedValue at arg 0 of command 2, an owned input put in a vector while borrowed
//> 0: test::m::inner_mut(Input(0));
//> 1: test::m::f_mut(Result(0));
//> 2: MakeMoveVec([Input(0)]);
//> 3: test::m::use_mut<u64>(Result(1));

//# programmable --sender A --inputs object(4,0)
// INVALID: CannotMoveBorrowedValue at arg 0 of command 1, an owned input deleted while its id is borrowed
//> 0: sui::object::borrow_id<test::m::Obj>(Input(0));
//> 1: test::m::delete(Input(0));
//> 2: sui::object::id_to_address(Result(0));

//# programmable --sender A --inputs object(5,0)
// INVALID: CannotMoveBorrowedValue at arg 0 of command 1, a shared input deleted while borrowed
//> 0: test::m::inner_mut(Input(0));
//> 1: test::m::delete(Input(0));
//> 2: test::m::use_mut<test::m::Inner>(Result(0));

//# programmable --sender A
// INVALID: CannotMoveBorrowedValue at arg 1 of command 2, the parent by value while the sibling reference from one call is live
//> 0: test::m::new();
//> 1: test::m::open(Result(0));
//> 2: test::m::close_and_delete(NestedResult(1,0), Result(0));
//> 3: test::m::use_mut<test::m::Inner>(NestedResult(1,1));

// --- receiving inputs ---

//# programmable --sender A --inputs object(6,0) receiving(7,0)
// INVALID: CannotMoveBorrowedValue at arg 1 of command 1, received while a `&mut` into the receiving input is live
//> 0: test::m::recv_mut(Input(1));
//> 1: test::m::receive(Input(0), Input(1));
//> 2: test::m::use_mut<sui::transfer::Receiving<test::m::Child>>(Result(0));

//# programmable --sender A --inputs object(6,0) receiving(7,0)
// INVALID: CannotMoveBorrowedValue at arg 1 of command 1, received while a `&` into the receiving input is live
//> 0: test::m::recv(Input(1));
//> 1: test::m::receive(Input(0), Input(1));
//> 2: test::m::use_imm<sui::transfer::Receiving<test::m::Child>>(Result(0));

//# programmable --sender A --inputs receiving(7,0)
// INVALID: CannotMoveBorrowedValue at arg 1 of command 0, `&` then by value in one call
//> 0: test::m::r_then_v(Input(0), Input(0));

//# programmable --sender A --inputs receiving(7,0)
// INVALID: ArgumentWithoutValue at arg 1 of command 0, by value then `&` in one call
//> 0: test::m::v_then_r(Input(0), Input(0));

// --- gas, coins, withdrawals ---

//# programmable --sender A --inputs @B
// INVALID: CannotMoveBorrowedValue at arg 0 of command 1, the gas coin transferred while borrowed
//> 0: test::m::id_mut<sui::coin::Coin<sui::sui::SUI>>(Gas);
//> 1: TransferObjects([Gas], Input(0));
//> 2: test::m::use_mut<sui::coin::Coin<sui::sui::SUI>>(Result(0));

//# programmable --sender A --inputs @B
// INVALID: CannotMoveBorrowedValue at arg 0 of command 1, send_funds takes the gas coin by value while borrowed
//> 0: test::m::id_mut<sui::coin::Coin<sui::sui::SUI>>(Gas);
//> 1: sui::coin::send_funds<sui::sui::SUI>(Gas, Input(0));
//> 2: test::m::use_mut<sui::coin::Coin<sui::sui::SUI>>(Result(0));

//# programmable --sender A --inputs @B
// INVALID: ArgumentWithoutValue at arg 0 of command 1, the gas coin borrowed after it was transferred
//> 0: TransferObjects([Gas], Input(0));
//> 1: test::m::id_mut<sui::coin::Coin<sui::sui::SUI>>(Gas);
//> 2: test::m::use_mut<sui::coin::Coin<sui::sui::SUI>>(Result(1));

//# programmable --sender A --inputs 1000 @A
// INVALID: CannotMoveBorrowedValue at arg 1 of command 2, a coin merged into a reference to itself
//> 0: SplitCoins(Gas, [Input(0)]);
//> 1: test::m::id_mut<sui::coin::Coin<sui::sui::SUI>>(Result(0));
//> 2: MergeCoins(Result(1), [Result(0)]);
//> 3: TransferObjects([Result(0)], Input(1));

//# programmable --sender A --inputs 1000 @A
// INVALID: CannotMoveBorrowedValue at arg 1 of command 1, a coin merged into itself by value
//> 0: SplitCoins(Gas, [Input(0)]);
//> 1: MergeCoins(Result(0), [Result(0)]);
//> 2: TransferObjects([Result(0)], Input(1));

//# programmable --sender A --inputs 100 @B
// INVALID: CannotMoveBorrowedValue at arg 1 of command 2, a coin merged away while borrowed
//> 0: SplitCoins(Gas, [Input(0), Input(0)]);
//> 1: test::m::id_mut<sui::coin::Coin<sui::sui::SUI>>(NestedResult(0,1));
//> 2: MergeCoins(NestedResult(0,0), [NestedResult(0,1)]);
//> 3: test::m::use_mut<sui::coin::Coin<sui::sui::SUI>>(Result(1));
//> 4: TransferObjects([NestedResult(0,0)], Input(1));

//# programmable --sender A --inputs 100 @B
// INVALID: CannotMoveBorrowedValue at arg 0 of command 2, a coin transferred while borrowed
//> 0: SplitCoins(Gas, [Input(0)]);
//> 1: test::m::id_mut<sui::coin::Coin<sui::sui::SUI>>(Result(0));
//> 2: TransferObjects([Result(0)], Input(1));
//> 3: test::m::use_mut<sui::coin::Coin<sui::sui::SUI>>(Result(1));

//# programmable --sender A --inputs withdraw<sui::balance::Balance<sui::sui::SUI>>(100) @B
// INVALID: CannotMoveBorrowedValue at arg 0 of command 1, a withdrawal redeemed while borrowed
//> 0: test::m::id_mut<sui::funds_accumulator::Withdrawal<sui::balance::Balance<sui::sui::SUI>>>(Input(0));
//> 1: sui::balance::redeem_funds<sui::sui::SUI>(Input(0));
//> 2: test::m::use_mut<sui::funds_accumulator::Withdrawal<sui::balance::Balance<sui::sui::SUI>>>(Result(0));

// --- upgrade caps and tickets ---

//# programmable --sender A --inputs object(2,1)
// INVALID: CannotMoveBorrowedValue at arg 0 of command 1, the cap by value while a reference into it is live
//> 0: test::m::id_mut<sui::package::UpgradeCap>(Input(0));
//> 1: sui::package::make_immutable(Input(0));
//> 2: test::m::use_mut<sui::package::UpgradeCap>(Result(0));

//# programmable --sender A --inputs object(2,1) 0u8 digest(q_2)
// INVALID: CannotMoveBorrowedValue at arg 0 of command 2, Upgrade consumes the ticket while a reference into it is live
//> 0: sui::package::authorize_upgrade(Input(0), Input(1), Input(2));
//> 1: test::m::id<sui::package::UpgradeTicket>(Result(0));
//> 2: Upgrade(q_2, [sui, std], q, Result(0));
//> 3: sui::package::ticket_policy(Result(1));
//> 4: sui::package::commit_upgrade(Input(0), Result(2));
