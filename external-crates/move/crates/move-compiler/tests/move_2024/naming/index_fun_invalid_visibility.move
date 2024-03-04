module 0x42::m {

    public struct A {}

    #[syntax(index)]
    public(package) fun index_a(a: &A): &A { a }

    public struct B {}

    #[syntax(index)]
    friend fun index_b(b: &B): &B { b }

    public struct C {}

    #[syntax(index)]
    fun index_c(c: &C): &C { c }

}
