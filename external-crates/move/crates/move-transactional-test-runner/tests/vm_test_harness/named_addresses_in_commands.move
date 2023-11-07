//# init --addresses A=42

//# run --args @A
script {
    fun main(a: &address) {
        assert!(*a == @42, 1000);
    }
}

//# publish
module A::M {
    struct Foo has key {
        x: u64,
    }

    public fun test(): Foo {
        Foo { x: 42 }
    }
}

//# run 42::M::test
