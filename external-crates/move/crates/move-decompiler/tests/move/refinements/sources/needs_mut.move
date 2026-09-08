// Exercises `needs_mut`, the per-binder `mut` inference the pretty printer runs just
// before printing. Each function pins one rule; the `@full` snapshot is the assertion,
// so a `mut` that appears or disappears shows up as a diff.

module refinements::needs_mut;

public enum E has copy, drop {
    A { x: u64 },
    B,
}

public struct S has copy, drop { v: u64, w: u64 }

// Read twice, never written: no `mut`.
#[allow(unused)]
public fun unassigned_let(n: u64): u64 {
    let x = double(n);
    x + x
}

// Initialized, then assigned once more: `mut`.
#[allow(unused)]
public fun reassigned_let(c: bool, n: u64): u64 {
    let mut x = n;
    if (c) { x = n + 1 };
    x + x
}

// Mutably borrowed: `mut`, even with no assignment.
#[allow(unused)]
public fun mut_borrowed_let(n: u64): u64 {
    let mut x = n;
    add_one(&mut x);
    x
}

// Uninitialized binder with one assignment per path: no `mut`, the assignment is the
// initializer. Two binders keep the pair from collapsing into `let x = if ...`.
#[allow(unused)]
public fun declare_single_assign_per_path(c: bool): u64 {
    let x;
    let y;
    if (c) { x = 1; y = 2 } else { x = 3; y = 4 };
    x + y
}

// The same binder assigned twice on one path: `mut`.
#[allow(unused)]
public fun declare_double_assign(c: bool): u64 {
    let mut x;
    let y;
    if (c) { x = 1; y = 2 } else { x = 3; y = 4 };
    if (c) { x = 5 };
    x + y
}

// The back edge repeats the assignment: `mut`.
#[allow(unused)]
public fun assign_reaching_back_edge(n: u64): u64 {
    let mut i = 0;
    while (i < n) { i = i + 1 };
    i
}

// Assigned once on the way out of the loop: no `mut` on the escaping value, `mut` on the
// counter the back edge repeats.
#[allow(unused)]
public fun assign_then_break(n: u64, m: u64): u64 {
    let x;
    let mut i = 0;
    loop {
        if (i >= n) { x = i; break };
        i = i + 1
    };
    if (x > m) { x - m } else { m - x }
}

// The same, from an inner loop breaking out of both.
#[allow(unused)]
public fun labeled_break_exits_both_loops(n: u64, m: u64): u64 {
    let x;
    let mut i = 0;
    'outer: loop {
        let mut j = 0;
        loop {
            if (i + j >= n) { x = i + j; break 'outer };
            if (j >= m) break;
            j = j + 1
        };
        i = i + 1
    };
    if (x > m) { x - m } else { m - x }
}

// A parameter handed out by mutable reference: `mut` in the signature.
#[allow(unused)]
public fun param_mut_borrow(mut n: u64): u64 {
    add_one(&mut n);
    n
}

// Unpack binders are single-assignment, so the `mut` lands on the binding that copies
// one out, never on the pattern.
#[allow(unused)]
public fun unpack_binding(s: S): u64 {
    let S { v: mut v, w } = s;
    add_one(&mut v);
    v + w
}

// Match-arm pattern binders are single-assignment for the same reason.
#[allow(unused)]
public fun arm_binding(e: E): u64 {
    match (e) {
        E::A { x: mut x } => {
            add_one(&mut x);
            x
        },
        E::B => 0,
    }
}

fun double(p: u64): u64 { p + p }

fun add_one(p: &mut u64) {
    *p = *p + 1;
}
