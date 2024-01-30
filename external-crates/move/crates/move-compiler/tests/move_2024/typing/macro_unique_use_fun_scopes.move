module a::m {
    public struct A() has drop;
    public struct B() has drop;
    public struct C() has drop;
    public struct D() has drop;
    public struct X() has drop;

    fun a(_: u64): A { A() }
    fun b(_: u64): B { B() }
    fun c(_: u64): C { C() }
    fun d(_: u64): D { D() }

    macro fun apply($x: u64, $f: |u64| -> u64): u64 {
        use fun d as u64.foo;
        let x = $x;
        (x.foo(): D);
        let res = $f({
            (x.foo(): D);
            x
        });
        (res.foo(): D);
        res
    }

    // we overload foo in each context
    // the type annotation tests that the correct foo is used
    use fun a as u64.foo;
    fun test() {
        apply!(
            {
                use fun b as u64.foo;
                (1u64.foo(): B);
                1
            },
            |x| {
                use fun c as u64.foo;
                (x.foo(): C);
                x
            }
        );
        apply!(
            {
                (1u64.foo(): A);
                1
            },
            |x| {
                (x.foo(): A);
                x
            }
        );
    }



}
