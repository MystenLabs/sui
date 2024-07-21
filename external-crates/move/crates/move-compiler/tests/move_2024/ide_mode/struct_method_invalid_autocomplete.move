#[allow(ide_path_autocomplete)]
module a::m {

    public struct A has copy, drop { }

    public struct B() has copy, drop;

    public fun t0(_s: A): u64 { abort 0 }
    public fun t1(_s: &A): u64 { abort 0 }
    public fun t2(_s: &A): u64 { abort 0 }

    public fun t3(_s: B): u64 { abort 0 }
    public fun t4(_s: &B): u64 { abort 0 }
    public fun t5(_s: &B): u64 { abort 0 }

    #[allow(ide_path_autocomplete)]
    public fun foo() {
        let _a = A {};
        let _b = B();
        let _tmp1 = _a.t7();
        let _tmp2 = _b.t8();
    }
}
