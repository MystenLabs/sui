module 0x42::m {

    public enum Maybe<T> has drop {
        Just(T),
        Nothing
    }

    fun test_00(x: Maybe<u64>): u64 {
        match (x) {
            just @ Maybe::Just(n) if (just == just)=> n,
            Maybe::Just(q) => q,
            Maybe::Nothing => 0
        }
    }

}
