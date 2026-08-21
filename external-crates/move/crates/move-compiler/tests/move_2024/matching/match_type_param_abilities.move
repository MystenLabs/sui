module 0x42::m;

public enum State has drop {
    On,
    Off,
}

public struct Inner<T> has drop { asset: T, state: State }

fun unconstrained<T>(i: Inner<T>): T {
    match (i) {
        Inner { asset, state: State::On } => asset,
        Inner { asset, state: State::Off } => asset,
    }
}

fun key_store<T: key + store>(i: &Inner<T>): bool {
    match (i) {
        Inner { asset: _, state: State::On } => true,
        Inner { asset: _, state: State::Off } => false,
    }
}

fun copy_drop<T: copy + drop>(i: Inner<T>): u64 {
    match (i) {
        Inner { asset: _, state: State::On } => 0,
        Inner { asset: _, state: _ } => 1,
    }
}
