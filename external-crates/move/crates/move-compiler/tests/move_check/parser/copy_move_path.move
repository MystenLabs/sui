module a::m {
    struct X has copy, drop {
        y: Y
    }
    struct Y has copy, drop {
        z: Z
    }
    struct Z has copy, drop {
        f: u64
    }
    fun test(x: X) {
        copy x.y.z;
        move x.y.z;
    }
}
