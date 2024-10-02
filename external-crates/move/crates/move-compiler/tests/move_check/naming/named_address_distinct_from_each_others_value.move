// These checks straddle a few different passes but
// Named addresses are no longer distinct from their value, even with a different name
// This is due to the package system mostly maintaining the value

module K::M {
    const C: u64 = 0;
    struct S {}
    public fun s(): S { S{} }
}

module K::Ex0 {
    friend K::M;
}

module K::Ex1 {
    use k::M;
    public fun ex(): K::M::S {
        k::M::C;
        k::M::s()
    }

    public fun ex2(): M::S {
        ex()
    }
}
