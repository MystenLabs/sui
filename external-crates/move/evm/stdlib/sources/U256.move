#[evm_arith]
module Evm::U256 {
    const MAX_U128: u128 = 340282366920938463463374607431768211455;

    native struct U256 has copy, drop, store;
    native public fun u256_from_words(hi: u128, lo: u128): U256;
    native public fun add(x: U256, y: U256): U256;
    native public fun sub(x: U256, y: U256): U256;
    native public fun mul(x: U256, y: U256): U256;
    native public fun div(x: U256, y: U256): U256;
    native public fun mod(x: U256, y: U256): U256;
    native public fun eq(x: U256, y: U256): bool;
    native public fun ne(x: U256, y: U256): bool;
    native public fun gt(x: U256, y: U256): bool;
    native public fun lt(x: U256, y: U256): bool;
    native public fun ge(x: U256, y: U256): bool;
    native public fun le(x: U256, y: U256): bool;
    native public fun shl(x: U256, y: u8): U256;
    native public fun shr(x: U256, y: u8): U256;


    public fun u256_from_u128(lo: u128): U256 {
        u256_from_words(0, lo)
    }

    public fun zero(): U256 {
        u256_from_words(0, 0)
    }

    public fun one(): U256 {
        u256_from_words(0, 1)
    }

    public fun max(): U256 {
        u256_from_words(MAX_U128, MAX_U128)
    }

    native public fun to_address(x: U256): address;
}
