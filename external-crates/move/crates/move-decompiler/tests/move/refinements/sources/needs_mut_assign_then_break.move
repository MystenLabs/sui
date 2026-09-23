// A value assigned on the way out of a loop never reaches the back edge, so it stays
// immutable; the counter the back edge does repeat takes `mut`. Reading the value twice
// afterwards keeps its binding from folding into the `return`.

module refinements::needs_mut_assign_then_break;

#[allow(unused)]
public fun assign_then_break(n: u64, m: u64): u64 {
    let x;
    let mut i = 0;
    loop {
        if (i >= n) { x = i; break };
        i = i + 1
    };
    if (x > m) { x - m } else { m - x }
}
