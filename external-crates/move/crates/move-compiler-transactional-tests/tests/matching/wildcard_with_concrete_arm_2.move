//# init --edition 2024.beta

//# publish
module 0x42::m {

    public enum A has drop { B, C }

    public fun make_b(): A { A::B }
    public fun make_c(): A { A::C }

    public fun test(a: &A, b: bool): u64 {
        match (a) {
            _ if (b) => 0,
            A::B => 1,
            _ => 2,
        }
    }
}

//# run
module 0x43::main {

    use 0x42::m;

    fun main() {
        let b = m::make_b();
        let c = m::make_c();
        assert!(m::test(&b, true) == 0, 0);
        assert!(m::test(&b, false) == 1, 1);
        assert!(m::test(&c, true) == 0, 2);
        assert!(m::test(&c, false) == 2, 3);
    }
}
