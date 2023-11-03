//# init --edition 2024.alpha

//# publish
module 0x42::m {

    public enum ABC<Q,R> has drop {
        A(Q,R),
        B,
        C(Q,R),
    }

    public enum QEither<L,R> has drop {
        Left(L),
        Right(R),
    }

    fun t0(in: ABC<u64, QEither<u64, u64>>): u64 {
        match (in) {
            ABC::C(x, QEither::Left(y)) => x + y,
            ABC::C(x, QEither::Right(y)) => x + y,
            ABC::A(b, QEither::Left(d)) => b + d,
            ABC::A(c, QEither::Right(e)) => c + e,
            ABC::B => 1,
        }
    }

}
