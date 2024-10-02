module Symbols::M8 {

    const MY_BOOL: bool = false;

    const PAREN: bool = (true);

    const BLOCK: bool = {true};

    const MY_ADDRESS: address = @0x70DD;

    const BYTES: vector<u8> = b"hello world";

    const HEX_BYTES: vector<u8> = x"DEADBEEF";

    const NUMS: vector<u16> = vector[1, 2];

    const RULE: bool = true && false;

    const CAP: u64 = 10 * 100 + 1;

    const SHIFTY: u8 = 1 << 1;

    const HALF_MAX: u128 = 340282366920938463463374607431768211455 / 2;

    const REM: u256 = 57896044618658097711785492504343953926634992332820282019728792003956564819968 % 654321;

    const USE_CONST: bool = EQUAL == false;

    const EQUAL: bool = 1 == 1;

    const ANOTHER_USE_CONST: bool = Symbols::M8::EQUAL == false;

    #[error]
    const ERROR_CONST: u64 = 42;

    public fun clever_assert() {
        assert!(ERROR_CONST == 42, ERROR_CONST);
    }

}
