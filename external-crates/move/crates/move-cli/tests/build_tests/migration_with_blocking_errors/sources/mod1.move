module A::mod1 {
    use A::mod0;

    public fun t0(x: u64): u64 {
        mod0::mod0::f();
        x
    }

}
