//# init --edition 2024.beta

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

    public fun run() {
        let a = ABC::A(1, QEither::Left(2));
        let b = ABC::A(1, QEither::Right(2));
        let c = ABC::C(1, QEither::Left(2));
        let d = ABC::C(1, QEither::Right(2));
        let e = ABC::B;
        assert!(t0(a) == 3);
        assert!(t0(b) == 3);
        assert!(t0(c) == 3);
        assert!(t0(d) == 3);
        assert!(t0(e) == 1);
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

//# run 0x42::m::run
