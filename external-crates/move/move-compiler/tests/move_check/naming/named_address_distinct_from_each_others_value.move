// These checks straddle a few different passes but
// Named addresses are no longer distinct from their value, even with a different name
// This is due to the package system mostly maintaining the value

module A::M {
    const C: u64 = 0;
    struct S {}
    public fun s(): S { S{} }
}

module A::Ex0 {
    friend B::M;
}

module A::Ex1 {
    use B::M;
    public fun ex(): B::M::S {
        B::M::C;
        B::M::s()
    }

    public fun ex2(): M::S {
        ex()
    }
}
