module 0x42::a {

    public enum Maybe<T> {
        Just(T),
        Nothing,
    }

    fun main(m: &Maybe<u64>) {
        match (m) {
            Maybe::Nothing<u64> => (),
            _x<u64> => (),
            Maybe::Just<u64>(_) => (),
        }
    }

}
