#[allow(ide_path_autocomplete)]
module a::m {

    public struct A<T>(T) has drop;
    public struct B<T>(A<T>) has drop;
    public struct C { c: u64, d: u64 } has drop;

    public fun for_a_0<T>(_a: &A<T>) {  }
    public fun for_a_1<T>(_a: &A<T>) {  }
    public fun for_b_0<T>(_b: &B<T>) {  }
    public fun for_b_1<T>(_b: &B<T>) {  }
    public fun for_c_0(_c: &C) {  }
    public fun for_c_1(_c: &C) {  }

    public fun test0(in: B<A<u64>>) {
        let _ = &in.0.0;
    }

    public fun test1(in: B<A<u64>>) {
        let _ = &in.0.0.0 ;
    }

    public fun test2(in: B<A<C>>) {
        let _ = &in.0.0.0.d ;
    }
}
