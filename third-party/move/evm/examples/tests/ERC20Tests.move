module Evm::ERC20Tests {
    use Evm::Evm::sender;
    use Evm::U256::{zero, one, u256_from_u128};
    use std::vector;
    use Evm::ERC20;
    use std::ascii::{string};

    const Alice: address = @0x8877;
    const Bob: address = @0x8888;

    #[test]
    fun test_create() {
        ERC20::create(string(vector::empty<u8>()), string(vector::empty<u8>()), one());
        assert!(ERC20::balanceOf(sender()) == one(), 100);
        assert!(ERC20::balanceOf(Alice) == zero(), 101);
        assert!(ERC20::balanceOf(Bob) == zero(), 102);
    }

    #[test]
    fun test_balance_of() {
        ERC20::create(string(vector::empty<u8>()), string(vector::empty<u8>()), one());
        assert!(ERC20::balanceOf(sender()) == one(), 103);
        assert!(ERC20::balanceOf(Alice) == zero(), 104);
        assert!(ERC20::balanceOf(Bob) == zero(), 105);
    }

    #[test]
    #[expected_failure(abort_code = 109)]
    fun test_transfer() {
        ERC20::create(string(vector::empty<u8>()), string(vector::empty<u8>()), u256_from_u128(7));
        ERC20::transfer(Alice, one());
        assert!(ERC20::balanceOf(sender()) == u256_from_u128(6), 106);
        assert!(ERC20::balanceOf(Alice) == one(), 107);
        assert!(ERC20::balanceOf(Bob) == zero(), 108);
        assert!(ERC20::balanceOf(sender()) == u256_from_u128(7), 109); // expected to fail
    }
}
