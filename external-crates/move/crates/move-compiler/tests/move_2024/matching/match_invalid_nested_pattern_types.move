// Test various ill-typed patterns in nested fields
module 0x42::m;

public enum State has drop {
    On,
    Off,
}

public struct Wrap has drop { b: bool }
public struct SWrap has drop { s: State }

fun bad_bool_lit(w: &Wrap): u64 {
    match (w) {
        Wrap { b: 5u64 } => 0,
        Wrap { b: true } => 1,
        Wrap { b: false } => 2,
    }
}

fun bad_bool_lit_unsaturated(w: &Wrap): u64 {
    match (w) {
        Wrap { b: 5u64 } => 0,
        Wrap { b: true } => 1,
        _ => 2,
    }
}

fun bad_enum_lit(s: &SWrap): u64 {
    match (s) {
        SWrap { s: 5u64 } => 0,
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
