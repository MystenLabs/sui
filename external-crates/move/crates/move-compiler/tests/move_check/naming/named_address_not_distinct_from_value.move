// These checks straddle a few different passes but
// Named addresses are no longer distinct from their value
// This is due to the package system mostly maintaining the value

module A::M {
    const C: u64 = 0;
    struct S {}
    public fun s(): S { S{} }
}

module A::Ex0 {
    friend 0x41::M;
}

module A::Ex1 {
    use 0x41::M;
    public fun ex(): 0x41::M::S {
        0x41::M::C;
        0x41::M::s()
    }

    public fun ex2(): M::S {
        ex()
    }
}
