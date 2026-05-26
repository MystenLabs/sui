// Regression test: deep nested match over a struct field whose type is a generic
// type parameter constrained to `key + store` must compile without an ICE.
// Previously, `TypeInner::Param` (or `Ref(false, Param)`) in fringe position fell
// through to the `ice_assert!` catch-all in `build_match_tree`, causing a compiler
// panic. After the fix, such types are treated as opaque wildcards.
module 0x2::M {
    public enum State has store, drop {
        Idle,
        Active { value: u64 },
    }

    public struct Inner<T: key + store> has store {
        asset: T,
        state: State,
    }

    public enum Outer<T: key + store> has store {
        Variant { inner: Inner<T> },
    }

    public fun extract_value<T: key + store>(outer: Outer<T>): u64 {
        match (outer) {
            Outer::Variant {
                inner: Inner { asset: _asset, state: State::Active { value } },
            } => value,
            Outer::Variant {
                inner: Inner { asset: _asset, state: State::Idle },
            } => 0,
        }
    }
}
