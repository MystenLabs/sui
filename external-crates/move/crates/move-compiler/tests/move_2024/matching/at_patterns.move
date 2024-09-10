module 0x42::m {

    public enum Maybe<T> has drop {
        Just(T),
        Nothing
    }

    fun test_00<T: drop>(x: Maybe<T>, other: Maybe<T>): Maybe<T> {
        match (x) {
            just @ Maybe::Just(_) => just,
            Maybe::Nothing => other
        }
    }

    fun test_01<T: drop>(x: Maybe<T>, other: Maybe<T>): Maybe<T> {
        match (x) {
            _just @ Maybe::Just(_) => _just,
            Maybe::Nothing => other
        }
    }

    fun test_02<T: drop>(x: Maybe<T>, other: Maybe<T>): Maybe<T> {
        match (x) {
            _just @ Maybe::Just(_) => other,
            Maybe::Nothing => other
        }
    }

    fun test_03<T: drop>(x: Maybe<T>, other: Maybe<T>): Maybe<T> {
        match (x) {
            _ @ Maybe::Just(_) => other,
            Maybe::Nothing => other
        }
    }

    fun test_04<T: drop>(x: Maybe<T>, other: Maybe<T>): Maybe<T> {
        match (x) {
            (x @ Maybe::Nothing) | (x @ Maybe::Just(_)) => other,
        }
    }

}
