//# init --edition 2024.alpha

//# publish
module 0x42::m {

    public enum Either<L,R> has drop {
        Left(L),
        Right(R),
    }

    fun t0(in: Either<u64, u64>): u64 {
        match (in) {
            Either::Left(y) => y,
            Either::Right(y) => y,
        }
    }

}
