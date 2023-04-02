module 0xc::M {

    public fun f(): u64 {
        7
    }

    public fun g(): u64 {
        12
    }

    public fun call_f(): u64 {
        f() + 0xa::M::f() + 0xb::M::f()
    }

    public fun call_g(): u64 {
        g() + 0xa::A::g() + 0xb::B::g()
    }
}
