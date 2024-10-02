#[allow(unused_field)]
module 0x6::phantoms {
    public struct A<phantom T> {}

    public struct B<phantom T1, phantom T2> {}

    public struct C<phantom T1, phantom T2, phantom T3> {
        a: A<T1>,
        b: B<T2, T3>
    }

    public struct D<phantom T1, T2: store> {
        v: vector<T2>
    }

    public struct E<T1, phantom T2: store, T3> {
        v1: vector<T1>,
        v2: vector<T3>
    }
}
