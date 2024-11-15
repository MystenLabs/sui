#[allow(ide_path_autocomplete)]
module a::m {

    public struct A has copy, drop {
        x: u64
    }

    public struct B has copy, drop {
        a: A
    }

    public struct C has copy, drop {
        b: B
    }

    public fun bar(_a: A) {}

    public fun foo() {
        let b = B { a: A { x: 0 } };
        let c = C { b: b };
        c.b.a;            // two dots that should trigger auto-completion
        c.b.a.bar();      // resolved method name
        c.b.a.b();        // unresolved method name
    }
}
