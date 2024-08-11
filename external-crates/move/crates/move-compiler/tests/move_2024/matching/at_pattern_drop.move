module 0x42::m {

    public enum Maybe<T> has drop {
        Just(T),
        Nothing
    }

    fun test_00<T>(x: Maybe<T>): Maybe<T> {
        match (x) {
            just @ Maybe::Just(_) => just,
            Maybe::Nothing => Maybe::Nothing
        }
    }

}
