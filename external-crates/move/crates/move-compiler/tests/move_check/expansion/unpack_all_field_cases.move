module 0x8675309::M {
    struct T {}
    struct S has copy, drop { f: u64, g: u64 }
    fun foo() {
        let f;
        let g;
        let s = S{ f: 0, g: 0};
        T {} = T{};
        T { } = T{};
        S { f, g } = copy s;
        S { f: 0, g: 0} = copy s;
        S { g: 0, f } = copy s;
        S { g, f: 0 } = copy s;
    }
}
