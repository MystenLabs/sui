// Exercises `negate_comparison`: `!(a op b)` collapses to `a op' b` for every comparison.

module refinements::negate_comparison;

#[allow(unused)]
public fun not_eq(a: u64, b: u64): bool { !(a == b) }

#[allow(unused)]
public fun not_neq(a: u64, b: u64): bool { !(a != b) }

#[allow(unused)]
public fun not_lt(a: u64, b: u64): bool { !(a < b) }

#[allow(unused)]
public fun not_gt(a: u64, b: u64): bool { !(a > b) }

#[allow(unused)]
public fun not_le(a: u64, b: u64): bool { !(a <= b) }

#[allow(unused)]
public fun not_ge(a: u64, b: u64): bool { !(a >= b) }
