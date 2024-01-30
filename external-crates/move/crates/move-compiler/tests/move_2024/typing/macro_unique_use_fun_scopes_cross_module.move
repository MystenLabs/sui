module a::m {
    public struct A() has drop;
    public struct B() has drop;
    public struct X() has drop;

    public fun a(_: u64): A { A() }
    public fun b(_: u64): B { B() }

    use fun a as u64.foo;

    public macro fun apply($x: u64, $f: |u64| -> u64): u64 {
        let x = $x;
        $f({
            (x.foo(): A);
            x
        });
        (x.foo(): A);
        {
            use fun b as u64.foo;
            let res = $f({
                (x.foo(): B);
                x
            });
            (res.foo(): B);
            res
        }
    }

}

module b::other {
    fun t() {
        // the use funs should resolve even though they are defined in another module/not in scope
        a::m::apply!(1, |x| x + 1);
    }
}
