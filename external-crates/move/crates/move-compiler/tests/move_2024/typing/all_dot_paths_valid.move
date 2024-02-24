module a::m {
    const ZED: Z = Z { f: 0 };
    const VEC: vector<u64> = vector[0];

    public struct X has copy, drop {
        y: Y
    }
    public struct Y has copy, drop {
        z: Z
    }
    public struct Z has copy, drop {
        f: u64
    }

    fun id(x: X): X { x }
    fun ref_unused(_x: &X) { }
    fun deref(x: &X): X { *x }

    fun all_copy(x: X) {
        copy x;
        copy x.y;
        copy x.y.z;
        copy x.y.z.f;
        copy VEC;
        copy ZED.f;
        (copy x).id();
    }

    fun all_move(x: X, x2: X) {
        move x;
        (move x2).id();
    }

    fun all_borrow(x: X) {
        &x;
        &x.y;
        &x.y.z;
        &x.y.z.f;
        &VEC;
        &ZED.f;
        (&x).deref();
        &x.id();
        &x.deref();
    }

    fun all_borrow_mut(mut x: X) {
        &mut x;
        &mut x.y;
        &mut x.y.z;
        &mut x.y.z.f;
        &mut VEC;
        &mut ZED.f;
        (&mut x).deref();
        &mut x.id();
        &mut x.deref();
    }

    fun all_use(x: X) {
        x;
        x.y;
        x.y.z;
        x.y.z.f;
        VEC;
        ZED.f;
        x.id();
    }
}
