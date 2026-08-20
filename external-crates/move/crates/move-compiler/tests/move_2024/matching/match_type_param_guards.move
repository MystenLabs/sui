// Guards interacting with a type-parameter column: a guard that uses the parameter binder,
// and guarded wild rows above an unguarded discriminating row (GH-25790 interplay).
module 0x42::m;

public enum State has drop {
    On,
    Off,
}

public struct Inner<T> has drop { asset: T, state: State }

fun check<T>(_x: &T): bool {
    true
}

fun guard_uses_param_binder<T>(i: Inner<T>): (T, u64) {
    match (i) {
        Inner { asset, state: State::On } if (check(asset)) => (asset, 0),
        Inner { asset, state: State::On } => (asset, 1),
        Inner { asset, state: State::Off } => (asset, 2),
    }
}

fun guarded_wilds_above<T>(i: &Inner<T>, flag: bool): u64 {
    match (i) {
        Inner { asset: _, state: _ } if (flag) => 0,
        Inner { asset: _, state: State::On } => 1,
        Inner { asset: _, state: _ } => 2,
    }
}
