// A parameter handed out by mutable reference takes `mut` in the signature.

module refinements::needs_mut_param;

#[allow(unused)]
public fun param_mut_borrow(mut n: u64): u64 {
    add_one(&mut n);
    n
}

fun add_one(p: &mut u64) { *p = *p + 1; }
