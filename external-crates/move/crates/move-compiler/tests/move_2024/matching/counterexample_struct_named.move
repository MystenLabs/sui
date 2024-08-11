module 0x42::m {

    public struct NTuple2<T,U> { fst: T, snd: U } has drop;

    fun t00(): u64 {
        match (NTuple2 { fst: 0, snd: 1 }) {
            NTuple2 { fst: 0, snd: _ } => 0, // invalid
        }
    }

    fun t01(): u64 {
        match (NTuple2 { fst: 0, snd: 1 }) {
            NTuple2 { fst: 0, snd: _ } => 0,
            NTuple2 { fst: 3, snd: _ } => 3
        }
    }

    fun t02(tup: NTuple2<NTuple2<u64, u64>, NTuple2<u64, u64>>): u64 {
        match (tup) {
            NTuple2 { fst: NTuple2 { fst: 1, snd: 2 }, snd: NTuple2 { fst: 3, snd: 4 } } => 1,
        }
    }

    fun t03(tup: NTuple2<bool, bool>): bool {
        match (tup) {
            NTuple2 { fst: true, snd: false } => true,
        }
    }

    fun t04(tup: NTuple2<bool, bool>): bool {
        match (tup) {
            NTuple2 { fst: true, snd: false } => true,
            NTuple2 { fst: false, snd: true } => true,
        }
    }

    fun t07(): u64 {
        match (NTuple2 { fst: 0, snd: 1 }) {
            NTuple2 { fst: 0, snd: _ } => 0,
            NTuple2 { fst: 3, snd: _ } => 3,
            NTuple2 { fst: 7, snd: _ } => 7,
            NTuple2 { fst: 4, snd: _ } => 4
        }
    }
}
