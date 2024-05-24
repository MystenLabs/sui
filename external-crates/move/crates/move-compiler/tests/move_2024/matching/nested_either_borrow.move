module 0x42::m {

    public enum Either<T,U> {
        Ok(T),
        Err(U)
    }


    public fun t(o: &Either<Either<u64, bool>, Either<u64, bool>>): u64 {
        match (o) {
            Either::Ok(Either::Ok(x)) => *x,
            Either::Ok(Either::Err(true)) => 1,
            Either::Err(Either::Ok(x)) => *x,
            Either::Err(Either::Err(true)) => 1,
            x => other(x)
        }
    }

    public fun other(_x: &Either<Either<u64, bool>, Either<u64, bool>>): u64 {
        0
    }

}
