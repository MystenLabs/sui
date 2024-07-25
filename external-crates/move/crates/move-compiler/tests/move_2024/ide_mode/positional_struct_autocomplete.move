#[allow(ide_path_autocomplete)]
module a::m {

    public struct A(u64) has copy, drop;

    public struct B(A) has copy, drop;

    public fun foo() {
        let _s = B(A(0));
        let _tmp1 = _s.;
        let _tmp2 = _s.0.;
    }
}
