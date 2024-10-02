//# init --edition 2024.beta

//# publish
module 0x42::m {

    public struct PTuple2<T,U>(T, U) has drop;

    public struct NTuple2<T,U> { fst: T, snd: U } has drop;

    public fun t00(): u64 {
        match (PTuple2(0, 1)) {
            PTuple2(x, _) => x
        }
    }

    public fun pt(a: bool, b: bool): PTuple2<bool, bool> {
        PTuple2(a, b)
    }

    public fun nt(fst: bool, snd: bool): NTuple2<bool, bool> {
        NTuple2 { fst, snd }
    }

    public fun positional_and(tup: PTuple2<bool, bool>): bool {
        match (tup) {
            PTuple2(true, true) => true,
            PTuple2(true, false) => false,
            PTuple2(false, true) => false,
            PTuple2(false, false) => false,
        }
    }

    public fun t01(): u64 {
        match (NTuple2 { fst: 0, snd : 1}) {
            NTuple2 { fst: x, snd: _ } => x
        }
    }

    public fun named_and(tup: NTuple2<bool, bool>): bool {
        match (tup) {
            NTuple2 { fst: true, snd: true } => true,
            NTuple2 { fst: true, snd: false } => false,
            NTuple2 { fst: false, snd: true } => false,
            NTuple2 { fst: false, snd: false } => false,
        }
    }
}

//# run
module 0x43::main {
    use 0x42::m;
    fun main() {
        assert!(m::t00() == 0);
        assert!(m::t01() == 0);

        // use fun m::positional_and as bool.positional_and;
        // use fun m::named_and as bool.named_and;

        assert!(m::pt(true, true).positional_and() == m::nt(true, true).named_and());
        assert!(m::pt(true, false).positional_and() == m::nt(true, false).named_and());
        assert!(m::pt(false, true).positional_and() == m::nt(false, true).named_and());
        assert!(m::pt(false, false).positional_and() == m::nt(false, false).named_and());

        assert!(m::pt(true, true).positional_and() == true);
        assert!(m::pt(true, false).positional_and() == false);
        assert!(m::pt(false, true).positional_and() == false);
        assert!(m::pt(false, false).positional_and() == false);
    }
}
