// Two type-parameter columns straddling a discriminating column.
module 0x42::m;

public enum State has drop {
    On,
    Off,
}

public struct Pair<T, U> has drop { a: T, state: State, b: U }

fun t<T, U>(p: &Pair<T, U>): bool {
    match (p) {
        Pair { a: _, state: State::On, b: _ } => true,
        Pair { a: _, state: State::Off, b: _ } => false,
    }
}

fun t_binders<T, U>(p: Pair<T, U>): (T, U) {
    match (p) {
        Pair { a, state: State::On, b } => (a, b),
        Pair { a, state: State::Off, b } => (a, b),
    }
}
