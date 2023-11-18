module a::m {
    const ZED: Z = Z { f: 0 };
    const V: vector<u64> = vector[0];

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
    fun deref(x: &X): X { *x }

    fun all_move(x: X) {
        move x.y;
        move x.y.z;
        move x.y.z.f;
        move V;
        move ZED.f;
        move x.id();
    }
}
