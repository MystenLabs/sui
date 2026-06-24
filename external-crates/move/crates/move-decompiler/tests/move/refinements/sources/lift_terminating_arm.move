// Exercises `lift_terminating_arm`: `let X = if (...) { abort } else { rhs }; ...use(X)...`
// hoists the abort out so the let-RHS isn't hidden inside a conditional.

module refinements::lift_terminating_arm;

#[allow(unused)]
const E_ZERO: u64 = 1;

#[allow(unused)]
public fun guard_abort(x: u64): u64 {
    let y = if (x == 0) { abort E_ZERO } else { x + 1 };
    y + 10
}

#[allow(unused)]
public fun guard_abort_in_else(x: u64): u64 {
    let y = if (x == 0) { 42 } else { abort E_ZERO };
    y + 10
}
