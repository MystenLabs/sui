module a::m {
    public struct X has copy, drop {
        y: Y
    }
    public struct Y has copy, drop {
        z: Z
    }
    public struct Z has copy, drop {
        f: u64
    }
    fun test(mut x: X) {
        x.y.z;
        copy x.y.z;
        &x.y.z;
        &mut x.y.z;
        move x.y.z;
    }
}
