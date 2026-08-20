// Type-parameter columns surfacing at several depths of a nested unpack.
module 0x42::m;

public enum State has drop {
    On,
    Off,
}

public struct L3<T> has drop { c: T, state: State }
public struct L2<T> has drop { b: T, l3: L3<T> }
public struct L1<T> has drop { a: T, l2: L2<T> }

fun t<T>(x: &L1<T>): u64 {
    match (x) {
        L1 { a: _, l2: L2 { b: _, l3: L3 { c: _, state: State::On } } } => 0,
        L1 { a: _, l2: L2 { b: _, l3: L3 { c: _, state: State::Off } } } => 1,
    }
}

fun t_partial<T>(x: &L1<T>): u64 {
    match (x) {
        L1 { a: _, l2: L2 { b: _, l3: L3 { c: _, state: State::On } } } => 0,
        L1 { a: _, l2: _ } => 1,
    }
}
