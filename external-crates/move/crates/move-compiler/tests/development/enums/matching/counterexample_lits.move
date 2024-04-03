module 0x42::m {

    public enum Tuple2<T,U> has drop {
        Ctor(T, U),
    }

    fun t0(): u64 {
        match (Tuple2::Ctor(0, 1)) {
            Tuple2::Ctor(0, _) => 0,
        }
    }

    fun t1(): u64 {
        match (Tuple2::Ctor(0, 1)) {
            Tuple2::Ctor(x, _) => x
        }
    }

    fun t2(tup: Tuple2<Tuple2<u64, u64>, Tuple2<u64, u64>>): u64 {
        match (tup) {
            Tuple2::Ctor(Tuple2::Ctor(1, 2), Tuple2::Ctor(3, 4)) => 1,
        }
    }

    fun t3(tup: Tuple2<bool, bool>): bool {
        match (tup) {
            Tuple2::Ctor(true, false) => true,
        }
    }

    fun t4(tup: Tuple2<bool, bool>): bool {
        match (tup) {
            Tuple2::Ctor(true, false) => true,
            Tuple2::Ctor(false, true) => true,
        }
    }

    fun and(tup: Tuple2<bool, bool>): bool {
        match (tup) {
            Tuple2::Ctor(true, true) => true,
            Tuple2::Ctor(true, false) => false,
            Tuple2::Ctor(false, true) => false,
            Tuple2::Ctor(false, false) => false,
        }
    }

    fun t6(): u64 {
        match (Tuple2::Ctor(0, 1)) {
            Tuple2::Ctor(0, _) => 0,
            Tuple2::Ctor(3, _) => 3
        }
    }

    fun t7(): u64 {
        match (Tuple2::Ctor(0, 1)) {
            Tuple2::Ctor(0, _) => 0,
            Tuple2::Ctor(3, _) => 3,
            Tuple2::Ctor(7, _) => 7,
            Tuple2::Ctor(4, _) => 4
        }
    }

}
