// Constructor and literal patterns against a type-parameter-typed field must be rejected in
// typing. This is the invariant that lets match compilation bind such columns without
// inspecting them.
module 0x42::m;

public enum State has drop {
    On,
    Off,
}

public struct Wrap has drop { n: u64 }

public struct Inner<T> has drop { asset: T, state: State }

fun bad_struct<T>(i: &Inner<T>): bool {
    match (i) {
        Inner { asset: Wrap { n: _ }, state: _ } => true,
        _ => false,
    }
}

fun bad_literal<T>(i: &Inner<T>): bool {
    match (i) {
        Inner { asset: 0, state: _ } => true,
        _ => false,
    }
}

fun bad_variant<T>(i: &Inner<T>): bool {
    match (i) {
        Inner { asset: State::On, state: _ } => true,
        _ => false,
    }
}
