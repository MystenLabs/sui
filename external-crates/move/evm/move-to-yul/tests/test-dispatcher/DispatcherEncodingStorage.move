#[evm_contract]
module 0x2::M {
    use std::vector;
    use Evm::U256::{Self, U256};
    use Evm::Evm::sign;

    struct T<Elem: drop> has drop, key { v: vector<Elem> }

    struct State has drop, key {
        s1: vector<u8>,
        s2: vector<U256>,
    }

    #[callable(sig=b"test_string() returns (string)")]
    fun test_string(): vector<u8> acquires T {
        let v = vector::empty();
        vector::push_back(&mut v, 65u8);
        move_to(&sign(@0x42), T { v });
        *&borrow_global<T<u8>>(@0x42).v
    }

    #[callable(sig=b"test_vec_u64() returns (string, uint256[])")]
    fun test_vec_u64(): (vector<u8>, vector<U256>) acquires State {
        let s1 = vector::empty();
        vector::push_back(&mut s1, 65u8);
        let s2 = vector::empty();
        vector::push_back(&mut s2, U256::u256_from_words(0,64));
        vector::push_back(&mut s2, U256::u256_from_words(0,65));
        move_to(&sign(@0x42), State { s1, s2 });
        (*&borrow_global<State>(@0x42).s1, *&borrow_global<State>(@0x42).s2)
    }

}
