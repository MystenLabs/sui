// Exercises `bool_if_simplify`: `if (...) { lit } else { lit/e }` collapses to `&&`/`||`/`!`.

module refinements::bool_if_simplify;

#[allow(unused)]
public fun and_left(a: u64, b: u64): bool {
    if (a == 0) { b > 0 } else { false }
}

#[allow(unused)]
public fun or_right(a: u64, b: u64): bool {
    if (a == 0) { true } else { b > 0 }
}

#[allow(unused)]
public fun ident(a: u64): bool {
    if (a == 0) { true } else { false }
}

#[allow(unused)]
public fun neg(a: u64): bool {
    if (a == 0) { false } else { true }
}
