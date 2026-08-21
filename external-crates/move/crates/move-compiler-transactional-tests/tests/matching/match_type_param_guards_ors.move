//# init --edition 2024.beta

//# publish
module 0x42::m {

    public enum State has drop, copy {
        On,
        Off,
    }

    public struct Inner<T> { asset: T, state: State }

    public fun make<T>(asset: T, on: bool): Inner<T> {
        let state = if (on) State::On else State::Off;
        Inner { asset, state }
    }

    // Guarded all-wild row above discriminating rows, all behind a type-parameter column.
    public fun guard_wild<T>(i: Inner<T>, flag: bool): (T, u64) {
        match (i) {
            Inner { asset, state: _ } if (flag) => (asset, 100),
            Inner { asset, state: State::On } => (asset, 0),
            Inner { asset, state: State::Off } => (asset, 1),
        }
    }

    // Or-expansion produces two rows sharing the type-parameter-column binder.
    public fun or_arms<T>(i: Inner<T>): T {
        match (i) {
            Inner { asset, state: State::On } | Inner { asset, state: State::Off } => asset,
        }
    }

    public fun mut_binder(i: Inner<u64>): u64 {
        match (i) {
            Inner { asset: mut a, state: State::On } => {
                a = a + 1;
                a
            },
            Inner { asset: a, state: State::Off } => a,
        }
    }
}

//# run
module 0x43::main {
    use 0x42::m;

    fun main() {
        let (a, tag) = m::guard_wild(m::make(7u64, true), true);
        assert!(a == 7 && tag == 100, 0);
        let (a, tag) = m::guard_wild(m::make(7u64, true), false);
        assert!(a == 7 && tag == 0, 1);
        let (a, tag) = m::guard_wild(m::make(7u64, false), false);
        assert!(a == 7 && tag == 1, 2);

        assert!(m::or_arms(m::make(9u64, true)) == 9, 3);
        assert!(m::or_arms(m::make(9u64, false)) == 9, 4);

        assert!(m::mut_binder(m::make(10u64, true)) == 11, 5);
        assert!(m::mut_binder(m::make(10u64, false)) == 10, 6);
    }
}
