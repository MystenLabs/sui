// Type-parameter columns before, after, and between discriminating siblings on the fringe.
module 0x42::m;

public enum State has drop {
    On,
    Off,
}

public struct Before<T> has drop { asset: T, state: State }
public struct After<T> has drop { state: State, asset: T }
public struct Between<T> has drop { s1: State, asset: T, s2: State }

fun t_before<T>(x: &Before<T>): bool {
    match (x) {
        Before { asset: _, state: State::On } => true,
        Before { asset: _, state: State::Off } => false,
    }
}

fun t_after<T>(x: &After<T>): bool {
    match (x) {
        After { state: State::On, asset: _ } => true,
        After { state: State::Off, asset: _ } => false,
    }
}

fun t_between<T>(x: &Between<T>): u64 {
    match (x) {
        Between { s1: State::On, asset: _, s2: State::On } => 0,
        Between { s1: State::On, asset: _, s2: State::Off } => 1,
        Between { s1: State::Off, asset: _, s2: _ } => 2,
    }
}
