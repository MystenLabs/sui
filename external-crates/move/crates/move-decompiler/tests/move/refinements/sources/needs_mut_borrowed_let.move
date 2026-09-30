// An initialized binder takes `mut` at one mutable borrow, with no assignment anywhere.

module refinements::needs_mut_borrowed_let;

#[allow(unused)]
public fun mut_borrowed_let(n: u64): u64 {
    let mut x = n;
    add_one(&mut x);
    x
}

fun add_one(p: &mut u64) { *p = *p + 1; }
