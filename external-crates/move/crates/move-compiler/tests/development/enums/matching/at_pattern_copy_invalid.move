module 0x42::m {

    public enum Maybe<T> has drop {
        Just(T),
        Nothing
    }

    fun test_00(x: Maybe<u64>): Maybe<u64> {
        match (x) {
            just @ Maybe::Just(x) => if (x > 0) { just } else { Maybe::Just(x * 2) },
            x @ Maybe::Nothing => x
        }
    }

}
