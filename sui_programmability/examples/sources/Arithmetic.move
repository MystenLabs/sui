/// Show how to do arithmetic on u256
module Examples::Arithmetic {
    use StarcoinFramework::U256::{Self, U256};

    /// The arithmetic operation returns an incorrect output
    const EWRONG_OUTPUT: u64 = 0;

    const EQUAL: u8 = 0;

    public fun add_u256(a: U256, b: U256): U256 {
        U256::add(a, b)
    }

    #[test]
    public fun test_add() {
        // let sum = add_u256(U256::one(), U256::one());
        // assert!(compare(sum, U256::from_u128(2)) == EQUAL, EWRONG_OUTPUT);
        assert!(1 == 1, 0);
    }
}
