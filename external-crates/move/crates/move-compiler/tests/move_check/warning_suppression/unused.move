// missing_phantom is currently the only unused

module 0x42::x {}

#[allow(unused)]
module 0x42::m {
    struct B<phantom T> {}
    struct S<T1, T2> { f: B<T1> }

    use 0x42::x;
    fun var(a: u64) {
        use 0x42::x;
        let x;
    }
    fun dead() {
        use 0x42::x;
        loop {};
        assert!(1 == 0u64, 0)
    }
    fun ab() {
        abort 0;
    }
    fun assgn(x: u64) {
        let y = 0u64;
        x = 1;
    }
}

module 0x42::n {
    struct B<phantom T> {}
    #[allow(unused)]
    struct S<T1, T2> { f: B<T1> }

    #[allow(unused)]
    fun dead(a: u64) {
        use 0x42::x;
        let y;
        let z = 0u64;
        a = 0;
        let x = abort 0;
        x + 1u64;
        abort 0;
    }
}
