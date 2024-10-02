module 0x42::m {

    public enum Maybe<T> has drop {
        Just(T),
        Nothing
    }

    fun test_00<T>(x: &mut Maybe<T>, other: &mut Maybe<T>): &mut Maybe<T> {
        match (x) {
            just @ Maybe::Just(_) => just,
            Maybe::Nothing => other
        }
    }

    fun test_01<T>(x: &mut Maybe<T>, other: &mut Maybe<T>): &mut Maybe<T> {
        match (x) {
            _just @ Maybe::Just(_) => _just,
            Maybe::Nothing => other
        }
    }

    fun test_02<T>(x: &mut Maybe<T>, other: &mut Maybe<T>): &mut Maybe<T> {
        match (x) {
            _just @ Maybe::Just(_) => other,
            Maybe::Nothing => other
        }
    }

    fun test_03<T>(x: &mut Maybe<T>, other: &mut Maybe<T>): &mut Maybe<T> {
        match (x) {
            _ @ Maybe::Just(_) => other,
            Maybe::Nothing => other
        }
    }

    fun test_04<T>(x: &mut Maybe<T>, other: &mut Maybe<T>): &mut Maybe<T> {
        match (x) {
            (x @ Maybe::Nothing) | (x @ Maybe::Just(_)) => other,
        }
    }

}
