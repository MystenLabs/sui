module a::a {

    public struct A has drop {}

    public fun foo(_a: A): u64 { 0 }

    public fun bar(): A { A {} }

}

module a::b {
    public struct B has drop {}

    public fun baz(): B { B {} }
}

module a::c {
    use a::{a::{A, Self, foo as f}, b::{Self as q, B, baz as bar}};

    fun use_a() {
        let _x: A = a::bar();
        let x: A = ::a::a::bar();
        let _y = f(x);
        let _g: q::B = bar();
        let _h: B = bar();
    }

}

module 0x42::d {
    use a::{a::{A, bar as foo}, a as g};

    fun use_a() {
        let a: A = foo();
        let _b: u64 = ::a::a::foo(a);
        let a: A = foo();
        let _c = g::foo(a);
    }

}
