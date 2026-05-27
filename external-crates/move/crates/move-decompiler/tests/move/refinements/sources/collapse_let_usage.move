// Exercises `collapse_let_usage`: `let X = e; head_use(X)` collapses to `head_use(e)`
// when `X` is used exactly once and at `head_use`'s leftmost-evaluated position.

module refinements::collapse_let_usage;

#[allow(unused)]
public fun head_call(x: u64): u64 {
    let y = x + 1;
    y + 100
}

#[allow(unused)]
public fun direct_return(x: u64): u64 {
    let y = x + 1;
    y
}

// Counter-example: `y` sits at args[1] of `+`, not the head. The refinement should leave
// the `let` in place.
#[allow(unused)]
public fun not_head(x: u64, z: u64): u64 {
    let y = x + 1;
    z + y
}
