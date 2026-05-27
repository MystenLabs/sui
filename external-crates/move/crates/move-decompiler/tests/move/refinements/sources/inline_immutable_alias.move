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
