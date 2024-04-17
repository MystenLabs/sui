module 0x42::m {
    public struct A<T> { x: T }
    public struct B<T>(T)
    public struct C<>()

    fun t00(s: A<B<u64>>): u64 {
        match (s) {
            A { x: B(x) } => x,
        }
    }

    fun t01(s: &A<B<u64>>): &u64 {
        match (s) {
            A { x: B(x) } => x,
        }
    }

    fun t02(s: &mut A<B<u64>>): &mut u64 {
        match (s) {
            A { x: B(x) } => x,
        }
    }

    fun t03(s: B<A<C>>): u64 {
        match (s) {
            B(A { x: C()}) => 0,
        }
    }

    fun t04(s: &B<A<C>>): u64 {
        match (s) {
            B(A { x: C()}) => 0,
        }
    }

    fun t05(s: &mut B<A<C>>): u64 {
        match (s) {
            B(A { x: C()}) => 0,
        }
    }

}
