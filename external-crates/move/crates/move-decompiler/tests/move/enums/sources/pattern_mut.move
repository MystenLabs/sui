// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

// `mut` inference on binders that are not plain `let`s. Reassignment in the source
// collapses during refinement, so only the mutable borrow reaches the printer as a
// `mut`; the rest pin that no spurious `mut` appears on an arm, unpack, or parameter
// binder.
module enums::pattern_mut;

public enum E has copy, drop {
    A { x: u64 },
    B,
}

public struct S has copy, drop { v: u64 }

// Reassignment inside the arm.
public fun rebind_arm_binding(e: E, bound: u64): u64 {
    match (e) {
        E::A { x: mut x } => {
            x = x + bound;
            x
        },
        E::B => 0,
    }
}

// A mutable borrow inside the arm.
public fun borrow_arm_binding(e: E): u64 {
    match (e) {
        E::A { x: mut x } => {
            add_one(&mut x);
            x
        },
        E::B => 0,
    }
}

// Reassignment of an unpack binder.
public fun rebind_unpack_binding(s: S): u64 {
    let S { v: mut v } = s;
    v = v + 1;
    v
}

// Reassignment of a parameter.
public fun rebind_param(mut n: u64): u64 {
    n = n + 1;
    n
}

fun add_one(n: &mut u64) {
    *n = *n + 1;
}
