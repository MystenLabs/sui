module 0x42::m {
    // A

    public struct A {}

    #[syntax(index)]
    fun index_a(a: &A): &A { a }

    #[syntax(index)]
    public fun index_a_mut(a: &mut A): &mut A { a }

    public fun make_a(): &mut A { abort 0 }

    // B

    public struct B {}

    #[syntax(index)]
    public fun index_b(b: &B): &B { b }

    #[syntax(index)]
    fun index_b_mut(b: &mut B): &mut B { b }

    public fun make_b(): &mut B { abort 0 }

    // C

    public struct C {}

    #[syntax(index)]
    fun index_c(c: &C): &C { c }

    #[syntax(index)]
    fun index_c_mut(c: &mut C): &mut C { c }

    public fun make_c(): &mut C { abort 0 }

    public fun test() {
        let a = make_a();
        let _a0 = &a[];
        let _a0 = &mut a[];

        let b = make_b();
        let _b0 = &b[];
        let _b0 = &mut b[];

        let c = make_c();
        let _c0 = &c[];
        let _c0 = &mut c[];
    }
}

module 0x42::n {
    use 0x42::m;

    public fun test() {
        let a = m::make_a();
        let _a0 = &mut a[];

        let b = m::make_b();
        let _b0 = &b[];
    }
}
