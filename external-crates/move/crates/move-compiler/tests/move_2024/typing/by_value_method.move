module a::m {
    public struct X() has copy, drop;
    public fun by_val(_: X) {}
    public fun by_ref(_: &X) {}
    public fun by_mut(_: &mut X) {}

    public struct Y { x: X }
    public fun drop_y(y: Y) { let Y { x: _ } = y; }

    fun example(y: Y) {
        y.x.by_val();
        y.drop_y();
    }
}
