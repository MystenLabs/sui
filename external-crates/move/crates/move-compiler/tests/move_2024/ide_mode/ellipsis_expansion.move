#[allow(ide_path_autocomplete,ide_dot_autocomplete)]
module a::m {

    public struct A(u64, u64) has copy, drop;
    public struct B has copy, drop {
        a: A,
        q: u64
    }

    public enum C {
        X(u64, u64),
        Y { y: u64, z: u64 }
    }

    public fun test(a: &A, b: &B, c: &C) {
        let A(_q, _d) = a;
        let A(.., _q) = a;
        let A(_q, ..) = a;
        let A(..) = a;
        let B { a: _a, q: _q } = b;
        let B { q: _q, .. } = b;
        let B { a: _a, .. } = b;
        let B { .. } = b;
        match (c) {
            C::X(_q, _z) => (),
            C::X(_q, ..) => (),
            C::X(.., _z) => (),
            C::X(..) => (),
            C::Y { y: _y, z: _z } => (),
            C::Y { y: _y, .. } => (),
            C::Y { .., y: _y } => (),
            C::Y { .., z: _z } => (),
            C::Y { .. } => ()
        };
    }
}
