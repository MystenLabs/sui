// Test exhaustiveness behavior when an ill-typed variant pattern's name collides with a
// variant of the subject's enum.
module 0x42::m;

public enum State has drop {
    On,
    Off,
}

public enum Mode has drop {
    On(u64),
    Third,
}

public struct SWrap has drop { s: State }

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
