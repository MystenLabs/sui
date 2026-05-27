// Exercises `simplify_borrow_deref`: the `*(&x)` round-trip should collapse to a plain
// read of `x` in decompiled output.

module refinements::simplify_borrow_deref;

#[allow(unused)]
public fun deref_local(x: u64): u64 {
    *(&x)
}

#[allow(unused)]
public fun deref_local_mut(mut x: u64): u64 {
    *(&mut x)
}

#[allow(unused)]
public fun deref_in_guard(b: bool): u64 {
    if (*(&b)) { 1 } else { 0 }
}
