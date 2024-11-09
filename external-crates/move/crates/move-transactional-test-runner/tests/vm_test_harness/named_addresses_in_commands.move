//# init --addresses A=43

//# run --args @A
module 0x42::m {
    fun main(a: &address) {
        assert!(*a == @43, 1000);
    }
}

//# publish
module A::M {
    struct Foo has key {
        x: u64,
    }

    public fun test(): Foo {
        Foo { x: 43 }
    }
}

//# run 43::M::test
