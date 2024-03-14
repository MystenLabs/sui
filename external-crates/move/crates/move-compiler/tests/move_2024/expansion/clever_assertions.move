module 0x42::m {
    #[error(foo)]
    const A1: vector<u8> = b"foo";

    #[error(0)]
    const A2: vector<u8> = b"foo";

    #[error]
    const A3: vector<u8> = b"Foo";

    #[error = 10]
    const A4: vector<u8> = b"Foo";

    #[error(code = 10, code = 11)]
    const A5: vector<u8> = b"Foo";

    #[error(code = 10, other = 11)]
    const A6: vector<u8> = b"Foo";

    #[error(other = 11)]
    const A7: vector<u8> = b"Foo";

    #[error(code = 0u8)]
    const A8: vector<u8> = b"Foo";

    #[error(code = 0u16)]
    const A9: vector<u8> = b"Foo";

    #[error(code = 0u32)]
    const A10: vector<u8> = b"Foo";

    #[error(code = 0u64)]
    const A11: vector<u8> = b"Foo";

    #[error(code = 0u128)]
    const A12: vector<u8> = b"Foo";

    #[error(code = 0u256)]
    const A13: vector<u8> = b"Foo";

    #[error(code = @0x0)]
    const A14: vector<u8> = b"Foo";

    #[error(code = std::vector)]
    const A15: vector<u8> = b"Foo";

    #[error(code = std::vector::foo)]
    const A16: vector<u8> = b"Foo";

    // out of bounds value
    #[error(code = 32768)]
    const A17: vector<u8> = b"Foo";

    #[error(code(code = 32768))]
    const A18: vector<u8> = b"Foo";

}
