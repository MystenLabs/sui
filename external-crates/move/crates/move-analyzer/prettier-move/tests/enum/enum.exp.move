// options:
// printWidth: 40

module tests::enum {
    public enum Test {
        A,
        B,
        C,
    }

    public enum Test<phantom C> {
        A(u8, u64),
        C(u8),
        B { a: u8, b: u64 },
    }

    fun use_enum() {
        let _local = tests::enum::test::A(
            10,
            1000,
        );
    }
}
