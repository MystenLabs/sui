module 0x42::m {

    public struct PTuple2<T,U>(T, U) has drop;

    fun t00(): u64 {
        match (PTuple2(0, 1)) {
            PTuple2(0, _) => 0, // invalid
        }
    }

    fun t01(): u64 {
        match (PTuple2(0, 1)) {
            PTuple2(0, _) => 0,
            PTuple2(3, _) => 3
        }
    }

    fun t02(tup: PTuple2<PTuple2<u64, u64>, PTuple2<u64, u64>>): u64 {
        match (tup) {
            PTuple2(PTuple2(1, 2), PTuple2(3, 4)) => 1,
        }
    }

    fun t03(tup: PTuple2<bool, bool>): bool {
        match (tup) {
            PTuple2(true, false) => true,
        }
    }

    fun t04(tup: PTuple2<bool, bool>): bool {
        match (tup) {
            PTuple2(true, false) => true,
            PTuple2(false, true) => true,
        }
    }

    fun t07(): u64 {
        match (PTuple2(0, 1)) {
            PTuple2(0, _) => 0,
            PTuple2(3, _) => 3,
            PTuple2(7, _) => 7,
            PTuple2(4, _) => 4
        }
    }
}
