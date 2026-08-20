// Ill-typed patterns in nested (field) positions survive typing as their original pattern
// form, not `ErrorPat`. Exhaustiveness analysis must tolerate them next to the type error
// rather than panicking (a non-bool literal in a bool column crashed the compiler).
module 0x42::m;

public enum State has drop {
    On,
    Off,
}

public struct Wrap has drop { b: bool }
public struct SWrap has drop { s: State }

fun bad_bool_lit(w: &Wrap): u64 {
    match (w) {
        Wrap { b: 5 } => 0,
        Wrap { b: true } => 1,
        Wrap { b: false } => 2,
    }
}

fun bad_bool_lit_unsaturated(w: &Wrap): u64 {
    match (w) {
        Wrap { b: 5 } => 0,
        Wrap { b: true } => 1,
        _ => 2,
    }
}

fun bad_enum_lit(s: &SWrap): u64 {
    match (s) {
        SWrap { s: 5 } => 0,
        SWrap { s: State::On } => 1,
        SWrap { s: State::Off } => 2,
    }
}

fun bad_ctor_in_bool(w: &Wrap): u64 {
    match (w) {
        Wrap { b: State::On } => 0,
        Wrap { b: _ } => 1,
    }
}

public enum Mode has drop {
    On(u64),
    Third,
}

// `Mode::On` collides with `State::On` by name; `Mode::Third` is not a `State` variant at
// all but its presence makes the `State` column look saturated.
fun foreign_variant_collision(s: &SWrap): u64 {
    match (s) {
        SWrap { s: Mode::On(5) } => 0,
        SWrap { s: State::On } => 1,
        SWrap { s: State::Off } => 2,
    }
}

fun foreign_variant_saturating(s: &SWrap): u64 {
    match (s) {
        SWrap { s: Mode::Third } => 0,
        SWrap { s: State::On } => 1,
        SWrap { s: State::Off } => 2,
    }
}

// KNOWN BAD BEHAVIOR: exhaustiveness keys constructors by bare variant name
// (`first_variant_ctors` / `specialize_variant`), so the ill-typed `Mode::On` stands in
// for the genuinely missing `State::On`. Harmless today only because the type error above
// already halts compilation. Two flavors; if either snapshot entry changes shape, variant
// matching became identity-aware: update these comments and keep the honest diagnostics.
//
// Flavor 1: the counterexample is garbage -- unit variant `State::On` is reported with a
// phantom positional field leaked from `Mode::On(u64)` ("When '_0' is not 5").
fun bogus_counterexample(s: &SWrap): u64 {
    match (s) {
        SWrap { s: Mode::On(5) } => 0,
        SWrap { s: State::Off } => 1,
    }
}

// Flavor 2: `Mode::On(_)` covers the phantom field entirely, so the missing `State::On`
// arm produces NO non-exhaustive error at all.
fun suppressed_nonexhaustive(s: &SWrap): u64 {
    match (s) {
        SWrap { s: Mode::On(_) } => 0,
        SWrap { s: State::Off } => 1,
    }
}
