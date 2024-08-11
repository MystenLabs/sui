//# init --edition 2024.beta

//# publish
module 0x42::m {

    public enum Maybe<T> has drop {
        Just(T),
        Nothing
    }

    public fun run() {
        let x = Maybe::Just(42);
        let y = Maybe::Nothing;
        let z = test_00<u64>(x);
        let w = test_00<u64>(y);
        assert!(z == Maybe::Just(42));
        assert!(w == Maybe::Nothing);
    }

    public fun test_00<T>(x: Maybe<T>): Maybe<T> {
        match (x) {
            just @ Maybe::Just(_) => just,
            Maybe::Nothing => Maybe::Nothing
        }
    }

}

//# run 0x42::m::run
