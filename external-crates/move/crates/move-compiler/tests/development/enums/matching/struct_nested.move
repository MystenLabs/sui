module 0x42::m {
    public struct A<T> { x: T }
    public struct B<T>(T)
    public struct C<>()

    fun t00(s: A<C>): C {
        match (s) {
            A { x } => x,
        }
    }

    fun t01(s: &A<C>): &C {
        match (s) {
            A { x } => x,
        }
    }

    fun t02(s: &mut A<C>): &mut C {
        match (s) {
            A { x } => x,
        }
    }

    fun t03(s: B<C>): C {
        match (s) {
            B(x) => x,
        }
    }

    fun t04(s: &B<C>): &C {
        match (s) {
            B(x) => x,
        }
    }

    fun t05(s: &mut B<C>): &mut C {
        match (s) {
            B(x) => x,
        }
    }

}
