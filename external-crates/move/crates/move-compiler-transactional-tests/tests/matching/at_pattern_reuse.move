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
        let z = test_00(x);
        let w = test_00(y);
        assert!(z == 42);
        assert!(w == 0);
    }

    fun test_00(x: Maybe<u64>): u64 {
        match (x) {
            just @ Maybe::Just(n) if (just == just)=> n,
            Maybe::Just(_q) => 1, // Impossible to hit unless == is broken
            Maybe::Nothing => 0
        }
    }

}

//# run 0x42::m::run
