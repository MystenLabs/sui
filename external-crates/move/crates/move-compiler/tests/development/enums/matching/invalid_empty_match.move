module 0x42::m {
    public enum Empty {
        One
    }

    fun test_00(e: &Empty) {
        match (e) {
        }
    }

    public enum Either<T,U> {
        Ok(T),
        Err(U)
    }

    fun test_01(e: &Either<Empty, u64>): u64 {
        match (e) {
        }
    }


}
