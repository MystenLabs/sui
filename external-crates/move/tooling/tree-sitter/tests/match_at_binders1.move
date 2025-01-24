module 0x42::m {
    fun test_04<T: drop>(x: Maybe<T>, other: Maybe<T>): Maybe<T> {
        match (x) {
            (x @ Maybe::Nothing) | (x @ Maybe::Just(_)) => other,
        }
    }
}
