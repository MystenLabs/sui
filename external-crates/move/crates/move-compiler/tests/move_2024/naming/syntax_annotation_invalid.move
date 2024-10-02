module 0x42::m {

    public struct A {}

    #[syntax]
    public fun index_a(a: &A): &A { a }

    #[syntax(index(A))]
    public fun index_mut_a(a: &mut A): &mut A { a }


    public struct B {}

    #[syntax = index]
    public fun index_b(b: &B): &B { b }

    #[syntax(index = foo)]
    public fun index_mut_b(b: &mut B): &mut B { b }

    public struct C {}

    #[syntax(index, index)]
    public fun index_c(c: &C): &C { c }

}
