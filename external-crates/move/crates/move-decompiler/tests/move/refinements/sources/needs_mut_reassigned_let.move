// An initialized binder takes `mut` at one later assignment.

module refinements::needs_mut_reassigned_let;

#[allow(unused)]
public fun reassigned_let(c: bool, n: u64): u64 {
    let mut x = n;
    if (c) { x = n + 1 };
    x + x
}
