#[allow(ide_path_autocomplete)]
module a::m {

    public struct A has copy, drop {
        x: u64
    }

    public struct B has copy, drop {
        a: A
    }

    public fun foo() {
        let _s = B { a: A { x: 0 } };
        let _tmp2 = _s.b.x;
    }
}
