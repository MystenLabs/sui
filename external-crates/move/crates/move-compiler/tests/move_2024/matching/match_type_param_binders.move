// Binder forms against a type-parameter column: plain, `mut`, `_`, `@`-chains, unused.
module 0x42::m;

public enum State has drop {
    On,
    Off,
}

public struct Inner<T> has drop { asset: T, state: State }

fun plain<T>(i: Inner<T>): T {
    match (i) {
        Inner { asset, state: State::On } => asset,
        Inner { asset, state: State::Off } => asset,
    }
}

fun mut_binder<T: drop>(i: Inner<T>, replacement: T): T {
    match (i) {
        Inner { asset: mut a, state: State::On } => {
            a = replacement;
            a
        },
        Inner { asset: a, state: State::Off } => a,
    }
}

fun at_chain<T>(i: &Inner<T>): bool {
    match (i) {
        Inner { asset: _a @ _, state: State::On } => true,
        Inner { asset: _, state: State::Off } => false,
    }
}

fun unused_binder<T>(i: &Inner<T>): bool {
    match (i) {
        Inner { asset: unused, state: State::On } => true,
        Inner { asset: _, state: State::Off } => false,
    }
}
