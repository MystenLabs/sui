module 0x42::a {
        
    struct A has drop {} 

    public fun foo(_a: A): u64 { 0 }

    public fun bar(): A { A {} }

}

module 0x42::b {
    struct B has drop {} 

    public fun baz(): B { B {} }
}

module 0x42::c {
    use 0x42::{a::{A, Self as q, foo as bar}, b::{Self as q, B, baz as bar}};

    fun use_a() {
        let x: A = q::bar();
        let _y = f(x);
        let _g: q::B = bar();
        let _h: B = bar();
    }

}

module 0x42::d {
    use 0x42::{a::{A, Self as q, foo as f}, a as g};

    fun use_a() {
        let x: A = g::bar();
        let _y = bar(x);
    }

}
