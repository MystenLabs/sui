// These checks straddle a few different passes but
// Named addresses are no longer distinct from their value
// This is due to the package system mostly maintaining the value

module A::M {
    const C: u64 = 0;
    struct S {}
    public fun s(): S { S{} }
}

module A::Ex0 {
    friend 0x42::M;
}

module A::Ex1 {
    use 0x42::M;
    public fun ex(): 0x42::M::S {
        0x42::M::C;
        0x42::M::s()
    }

    public fun ex2(): M::S {
        ex()
    }
}
