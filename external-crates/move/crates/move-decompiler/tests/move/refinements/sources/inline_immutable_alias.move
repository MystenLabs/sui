// Exercises `inline_immutable_alias`: `let X = Y;` where `Y` is never written collapses
// even when `Y` is read in many other places. Earlier `inline_single_use_bindings` only
// fired for `let X = Y;` with `reads(Y) == 1`; here `Y` (`p`/`q`) is read elsewhere too.

module refinements::inline_immutable_alias;

#[allow(unused)]
public fun stage(p: u64, q: u64): u64 {
    let a = p;
    let b = q;
    let c = p;
    let d = q;
    a + b + c + d
}

// Regression: when the source slot is mut-borrowed elsewhere (here directly via `&mut x`
// passed into `add_one`), the binding must *not* be inlined. The mut borrow hands out a
// reference through which the slot can be reassigned, so substituting `y → x` would let
// the eventual read see the post-mutation value instead of the binding-time snapshot.
#[allow(unused)]
public fun mut_borrow_blocks_inline(): u64 {
    let mut x = 0;
    let y = x;
    add_one(&mut x);
    y
}

fun add_one(p: &mut u64) {
    *p = *p + 1;
}

