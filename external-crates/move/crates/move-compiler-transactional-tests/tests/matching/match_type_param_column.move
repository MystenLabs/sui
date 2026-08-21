//# init --edition 2024.beta

//# publish
module 0x42::m {

    public enum State has drop, copy {
        On,
        Off,
    }

    // No abilities: stresses the binding discipline for the type-parameter column.
    public struct NoAbilities { }

    public struct Inner<T> { asset: T, state: State }

    public struct Outer<T> { inner: Inner<T> }

    public fun make<T>(asset: T, on: bool): Outer<T> {
        let state = if (on) State::On else State::Off;
        Outer { inner: Inner { asset, state } }
    }

    public fun take<T>(o: Outer<T>): (T, u64) {
        match (o) {
            Outer { inner: Inner { asset, state: State::On } } => (asset, 0),
            Outer { inner: Inner { asset, state: State::Off } } => (asset, 1),
        }
    }

    public fun peek<T>(o: &Outer<T>): u64 {
        match (o) {
            Outer { inner: Inner { asset: _, state: State::On } } => 0,
            Outer { inner: Inner { asset: _, state: State::Off } } => 1,
        }
    }

    public fun flip<T>(o: &mut Outer<T>): u64 {
        match (o) {
            Outer { inner: Inner { asset: _, state } } if (*state == State::On) => {
                *state = State::Off;
                0
            },
            Outer { inner: Inner { asset: _, state } } => {
                *state = State::On;
                1
            },
        }
    }

    public fun na(): NoAbilities {
        NoAbilities { }
    }

    public fun consume(x: NoAbilities) {
        let NoAbilities { } = x;
    }
}

//# run
module 0x43::main {
    use 0x42::m;

    fun main() {
        // A no-abilities asset flows through the match intact in all three subject modes.
        let mut o = m::make(m::na(), true);
        assert!(m::peek(&o) == 0, 0);
        assert!(m::flip(&mut o) == 0, 1);
        assert!(m::peek(&o) == 1, 2);
        let (asset, tag) = m::take(o);
        assert!(tag == 1, 3);
        m::consume(asset);

        let o = m::make(42u64, false);
        assert!(m::peek(&o) == 1, 4);
        let (asset, tag) = m::take(o);
        assert!(tag == 1, 5);
        assert!(asset == 42, 6);
    }
}
