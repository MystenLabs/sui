module 0x42::a {

    public enum Maybe<T> {
        Just(T),
        Nothing,
    }

    fun main(m: &Maybe<u64>) {
        match (m) {
            mut Maybe::Nothing => (),
            mut x<u64> => (),
            mut Maybe<u64>::Just(_) => (),
        }
    }

}
