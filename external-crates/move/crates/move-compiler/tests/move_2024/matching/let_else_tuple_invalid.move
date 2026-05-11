// Tuple patterns in `let ... else` are not supported: a tuple pattern would
// destructure a tuple value, but tuples can't appear as `match` subjects in
// our pattern compiler, and `let ... else` lowers through match compilation.
// This test pins the rejection.
module 0x42::m {

    public enum O<T> has drop { S(T), N }

    fun two(a: O<u64>, b: O<u64>): u64 {
        let (O::S(x), O::S(y)) = (a, b) else { return 0 };
        x + y
    }

}
