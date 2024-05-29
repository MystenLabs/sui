module a::m {
    public struct ByValue() has copy, drop;
    public struct ByImm() has copy, drop;
    public struct ByMut() has copy, drop;

    use fun bv_foo as ByValue.foo;
    fun bv_foo(_: ByValue) {}

    use fun bi_foo as ByImm.foo;
    fun bi_foo(_: &ByImm) {}

    use fun bm_foo as ByMut.foo;
    fun bm_foo(_: &mut ByMut) {}

    // this is "duck typing" in the sense that this macro can be called only by those
    // types that "walk and talk like a duck"
    macro fun call_foo<$T>($x: $T) {
        let x = $x;
        x.foo()
    }

    fun t() {
        call_foo!(ByValue());
        call_foo!(&ByImm());
        call_foo!(&mut ByMut());
    }
}
