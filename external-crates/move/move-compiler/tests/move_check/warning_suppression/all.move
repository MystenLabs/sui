module 0x42::x {}

#[allow(all)]
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
        assert!(1 == 0, 0)
    }
    fun ab() {
        abort 0;
    }
    fun assgn(x: u64) {
        let y = 0;
        x = 1;
    }
}

module 0x42::n {
    struct B<phantom T> {}
    #[allow(all)]
    struct S<T1, T2> { f: B<T1> }

    #[allow(all)]
    fun dead(a: u64) {
        use 0x42::x;
        let y;
        let z = 0;
        a = 0;
        let x = abort 0;
        x + 1;
        abort 0;
    }
}
