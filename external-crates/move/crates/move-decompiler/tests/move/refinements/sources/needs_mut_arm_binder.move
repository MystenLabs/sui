// Match-arm pattern binders are single-assignment for the same reason as unpack binders.

module refinements::needs_mut_arm_binder;

public enum E has copy, drop {
    A { x: u64 },
    B,
}

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

fun add_one(p: &mut u64) { *p = *p + 1; }
