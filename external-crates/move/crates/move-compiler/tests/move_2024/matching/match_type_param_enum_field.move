module 0x42::m;

public enum State has drop {
    On,
    Off,
}

public enum Holder<T> has drop {
    V { asset: T, state: State },
    Empty,
}

fun t<T>(h: &Holder<T>): u64 {
    match (h) {
        Holder::V { asset: _, state: State::On } => 0,
        Holder::V { asset: _, state: State::Off } => 1,
        Holder::Empty => 2,
    }
}

fun t_value<T: drop>(h: Holder<T>, default: T): T {
    match (h) {
        Holder::V { asset, state: State::On } => asset,
        Holder::V { asset, state: State::Off } => asset,
        Holder::Empty => default,
    }
}
