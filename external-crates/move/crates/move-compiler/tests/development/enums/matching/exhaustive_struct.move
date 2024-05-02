module 0x42::m {

    public struct PTuple2<T,U>(T, U) has drop;

    public struct NTuple2<T,U> { fst: T, snd: U } has drop;

    fun t00(): u64 {
        match (PTuple2(0, 1)) {
            PTuple2(x, _) => x
        }
    }

    fun positional_and(tup: PTuple2<bool, bool>): bool {
        match (tup) {
            PTuple2(true, true) => true,
            PTuple2(true, false) => false,
            PTuple2(false, true) => false,
            PTuple2(false, false) => false,
        }
    }

    fun t01(): u64 {
        match (NTuple2 { fst: 0, snd : 1}) {
            NTuple2 { fst: x, snd: _ } => x
        }
    }

    fun named_and(tup: NTuple2<bool, bool>): bool {
        match (tup) {
            NTuple2 { fst: true, snd: true } => true,
            NTuple2 { fst: true, snd: false } => false,
            NTuple2 { fst: false, snd: true } => false,
            NTuple2 { fst: false, snd: false } => false,
        }
    }
}
