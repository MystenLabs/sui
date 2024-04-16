module 0x42::m {
    public struct X(u64, u64, bool)
    public struct Y {
        x: u64,
        y: u64,
        z: bool,
    }

    public fun f0(x: Y) {
        let Y{..} = x;
    }

    public fun f1(x: Y): u64 {
        let Y{x, ..} = x;
        x
    }

    public fun f2(x: Y): u64 {
        let Y{x, z, .. } = x;
        if (z) x else 0
    }

    public fun f3(x: Y): u64 {
        let Y{x, z, y, .. } = x;
        if (z) x + y else 0
    }

    public fun g0(x: X) {
        let X(..) = x;
    }

    public fun g1(x: X): u64 {
        let X(x, ..) = x;
        x
    }

    public fun g2(x: X): u64 {
        let X(x, .., z) = x;
        if (z) x else 0
    }

    public fun g3(x: X): u64 {
        let X(x, y, z, ..) = x;
        if (z) x + y else 0
    }
}
