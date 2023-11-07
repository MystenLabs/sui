//# publish
module 0x42::C {
    struct T {}
    public fun foo(): T {
        T{}
    }
}

//# publish
// names used to try to force an ordering of depedencies
module 0x42::B {
    public fun foo(): 0x42::C::T {
        0x42::C::foo()
    }
}

//# publish
module 0x42::A {
    struct T {
        t_b: 0x42::C::T,
        t_c: 0x42::C::T,
    }
    public fun foo(): T {
        T {
            t_c: 0x42::C::foo(),
            t_b: 0x42::B::foo()
        }
    }
}
