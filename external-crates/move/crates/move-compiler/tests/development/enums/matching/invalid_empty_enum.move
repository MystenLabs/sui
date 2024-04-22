module 0x42::m {
    public enum Empty {

    }

    fun test_01(e: &Empty): u64 {
        match (e) {
            _ => 5
        }
    }

    public enum Either<T,U> {
        Ok(T),
        Err(U)
    }

    fun test_02(e: &Either<Empty, u64>): u64 {
        match (e) {
            Either::Ok(_) => 5,
            Either::Err(_) => 10
        }
    }

}
