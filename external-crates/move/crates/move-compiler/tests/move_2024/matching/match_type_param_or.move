module 0x42::m;

public enum State has drop {
    On,
    Off,
}

public struct Inner<T> has drop { asset: T, state: State }

fun top_level_or<T>(i: Inner<T>): T {
    match (i) {
        Inner { asset, state: State::On } | Inner { asset, state: State::Off } => asset,
    }
}

fun field_or<T>(i: Inner<T>): T {
    match (i) {
        Inner { asset, state: State::On | State::Off } => asset,
    }
}
