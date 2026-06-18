// Exercises `simplify_zero_compare`: when the source writes the literal on the *left* of a
// comparison, the bytecode emits `Value op Variable`. The refinement swaps the args and
// flips the op so the literal lands on the right, matching the conventional form.

module refinements::simplify_zero_compare;

#[allow(unused)]
public fun lit_eq(x: u64): bool { 0 == x }

#[allow(unused)]
public fun lit_neq(x: u64): bool { 0 != x }

#[allow(unused)]
public fun lit_lt(x: u64): bool { 0 < x }

#[allow(unused)]
public fun lit_gt(x: u64): bool { 0 > x }

#[allow(unused)]
public fun lit_le(x: u64): bool { 0 <= x }

#[allow(unused)]
public fun lit_ge(x: u64): bool { 0 >= x }
