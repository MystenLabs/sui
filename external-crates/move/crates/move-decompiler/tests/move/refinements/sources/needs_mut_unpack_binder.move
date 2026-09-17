// Unpack binders are single-assignment, so `mut` lands on the binding that copies a field
// out, never on the pattern.

module refinements::needs_mut_unpack_binder;

public struct S has copy, drop { v: u64, w: u64 }

#[allow(unused)]
public fun unpack_binding(s: S): u64 {
    let S { v: mut v, w } = s;
    add_one(&mut v);
    v + w
}

fun add_one(p: &mut u64) { *p = *p + 1; }
