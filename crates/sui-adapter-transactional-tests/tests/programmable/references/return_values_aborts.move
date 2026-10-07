// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

// A reference returned with no reference sources, or a `&mut` returned from only immutable
// sources, is accepted by the static checks and borrows nothing. The only Move body for such a
// signature aborts, so every task here fails at runtime inside the callee.

//# init --addresses test=0x0 --accounts A --enable-feature-flags allow_references_in_ptbs

//# publish
module test::m;

public fun boom_mut(): &mut u64 { abort 7 }
public fun boom_imm(): &u64 { abort 8 }
public fun launder(_: &u64): &mut u64 { abort 9 }
public fun id_mut<T>(t: &mut T): &mut T { t }
public fun two_mut(_: &mut u64, _: &mut u64) {}
public fun write(r: &mut u64, v: u64) { *r = v }
public fun use_imm<T>(_: &T) {}

//# programmable --inputs 1
// ABORTS: a `&mut` with no sources is writable, boom_mut aborts with code 7
//> 0: test::m::boom_mut();
//> 1: test::m::write(Result(0), Input(0));

//# programmable
// ABORTS: a `&` with no sources, boom_imm aborts with code 8
//> 0: test::m::boom_imm();
//> 1: test::m::use_imm<u64>(Result(0));

//# programmable
// ABORTS: an unused sourceless result still runs the call, boom_mut aborts with code 7
//> 0: test::m::boom_mut();

//# programmable
// ABORTS: two distinct sourceless `&mut` results passed together, boom_mut aborts with code 7
//> 0: test::m::boom_mut();
//> 1: test::m::boom_mut();
//> 2: test::m::two_mut(Result(0), Result(1));

//# programmable --inputs 1 2
// ABORTS: a `&mut` from only immutable arguments is writable, launder aborts with code 9
//> 0: test::m::launder(Input(0));
//> 1: test::m::write(Result(0), Input(1));

//# programmable --inputs 0 1
// ABORTS: a laundered `&mut` does not borrow its immutable source, so a `&mut` of the source can join it, launder aborts with code 9
//> 0: test::m::launder(Input(0));
//> 1: test::m::id_mut<u64>(Input(0));
//> 2: test::m::two_mut(Result(0), Result(1));

//# programmable --inputs 0 1
// ABORTS: writing the source while the laundered `&mut` is live, launder aborts with code 9
//> 0: test::m::launder(Input(0));
//> 1: test::m::id_mut<u64>(Input(0));
//> 2: test::m::write(Result(1), Input(1));
//> 3: test::m::write(Result(0), Input(1));
