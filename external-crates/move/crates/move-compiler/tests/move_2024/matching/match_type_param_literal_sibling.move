// A type-parameter column ahead of a literal-switch column.
module 0x42::m;

public struct Val<T> has drop { asset: T, value: u64 }

fun t<T>(v: &Val<T>): u64 {
    match (v) {
        Val { asset: _, value: 0 } => 0,
        Val { asset: _, value: 1 } => 1,
        Val { asset: _, value: n } => *n,
    }
}

public struct Flag<T> has drop { asset: T, flag: bool }

fun t_bool<T>(f: &Flag<T>): u64 {
    match (f) {
        Flag { asset: _, flag: true } => 0,
        Flag { asset: _, flag: false } => 1,
    }
}
