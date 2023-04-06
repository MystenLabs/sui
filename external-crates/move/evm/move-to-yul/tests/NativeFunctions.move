// Tests native functions.
// dep: ../stdlib/sources/Evm.move
// dep: ../stdlib/sources/U256.move
#[evm_contract]
module 0x2::NativeFunctions {
    use Evm::Evm::{Self, abort_with, to_string, concat};
    use Evm::U256::{zero, one, u256_from_u128, u256_from_words};
    use std::signer;

    #[callable]
    fun call_native_functions() {
        let _ = Evm::blockhash(one());
        let _ = Evm::block_basefee();
        let _ = Evm::block_chainid();
        let _ = Evm::block_coinbase();
        let _ = Evm::block_difficulty();
        let _ = Evm::block_gaslimit();
        let _ = Evm::block_number();
        let _ = Evm::block_timestamp();
        let _ = Evm::gasleft();
        let _ = Evm::msg_data();
        let _ = Evm::msg_sender();
        let _ = Evm::msg_sig();
        let _ = Evm::msg_value();
        let _ = Evm::tx_gasprice();
        let _ = Evm::tx_origin();
    }

    #[evm_test]
    fun test_signer_address_of() {
        let s = Evm::sign(@0x42);
        assert!(signer::address_of(&s) == @0x42, 101);
    }

    #[evm_test]
    fun test_abort() {
        abort_with(b"error message");
    }

    #[evm_test]
    fun test_to_string() {
        assert!(to_string(zero()) == b"0", 101);
        assert!(to_string(one()) == b"1", 102);
        assert!(to_string(u256_from_u128(42)) == b"42", 103);
        assert!(to_string(u256_from_u128(7008)) == b"7008", 104);
        assert!(to_string(u256_from_words(1, 2)) == b"340282366920938463463374607431768211458", 105);
    }

    #[evm_test]
    fun test_concat() {
        assert!(concat(b"", b"") == b"", 100);
        assert!(concat(b"1", b"2") == b"12", 101);
        assert!(concat(b"", b"abc") == b"abc", 102);
        assert!(concat(concat(b"a", b"bc"), b"de") == b"abcde", 103);
        assert!(concat(b"test", b"") == b"test", 104);
    }
}
