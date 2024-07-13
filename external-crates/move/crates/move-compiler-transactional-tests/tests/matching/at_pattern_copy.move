//# init --edition 2024.beta

//# publish
module 0x42::m {

    public enum Maybe<T> has copy, drop {
        Just(T),
        Nothing
    }
    
    public fun run() {
        let x = Maybe::Just(42);
        let y = Maybe::Nothing;
        let z = Maybe::Just(0);
        let a = test_00(x);
        let b = test_00(y);
        let c = test_00(z);
        assert!(a == Maybe::Just(42 * 2));
        assert!(b == Maybe::Nothing);
        assert!(c == Maybe::Just(0));
    }

    public fun test_00(x: Maybe<u64>): Maybe<u64> {
        match (x) {
            just @ Maybe::Just(x) => if (x == 0) { just } else { Maybe::Just(x * 2) },
            x @ Maybe::Nothing => x
        }
    }
}

//# run 0x42::m::run
