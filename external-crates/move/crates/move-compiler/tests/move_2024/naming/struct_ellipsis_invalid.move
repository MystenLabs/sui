module 0x42::m {
    public struct X()
    public struct Y{}

    public fun f0(x: Y) {
        let Y(..) = x;
    }

    public fun f1(x: Y) {
        let Y(x, ..) = x;
    }

    public fun f2(x: Y) {
        let Y() = x;
    }

    public fun g0(x: X) {
        let X{..} = x;
    }

    public fun g1(x: X) {
        let X{x, ..} = x;
    }

    public fun g2(x: X) {
        let X{} = x;
    }
}
