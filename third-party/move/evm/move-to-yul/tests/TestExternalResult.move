#[evm_contract]
module 0x2::M {
    use Evm::ExternalResult::{Self, ExternalResult};
    use Evm::U256::{Self, U256};
    use std::vector;

    #[evm_test]
    fun extract_value(){
        let v2 = pack_value(100u64);
        assert!(ExternalResult::is_ok(&v2), 100);
        assert!(ExternalResult::unwrap(v2) == 100, 101);
    }

    #[evm_test]
    fun extract_err_data(){
        let v = vector::empty<u8>();
        vector::push_back(&mut v, 42u8);
        let v2 = pack_err_data<u64>(v);
        assert!(ExternalResult::is_err_data(&v2), 102);
        let v3 = ExternalResult::unwrap_err_data(v2);
        assert!(vector::length(&v3) == 1, 103);
        assert!(*vector::borrow(&v3, 0) == 42, 104);
    }

    #[evm_test]
    fun extract_err_reason() {
        let v = vector::empty<u8>();
        vector::push_back(&mut v, 42u8);
        let v2 = pack_err_reason<u64>(v);
        assert!(ExternalResult::is_err_reason(&v2), 105);
        let v3 = ExternalResult::unwrap_err_reason(v2);
        assert!(vector::length(&v3) == 1, 106);
        assert!(*vector::borrow(&v3, 0) == 42, 107);
    }

    #[evm_test]
    fun extract_panic_code() {
        let v2 = pack_panic_code<u64>(U256::u256_from_u128(42));
        assert!(ExternalResult::is_panic(&v2), 108);
        let v = ExternalResult::unwrap_panic<u64>(v2);
        assert!(v == U256::u256_from_u128(42), 109);
    }

    fun pack_value<T>(v: T): ExternalResult<T> {
        ExternalResult::ok<T>(v)
    }

    fun pack_err_data<T>(v: vector<u8>): ExternalResult<T> {
        ExternalResult::err_data(v)
    }

    fun pack_err_reason<T>(v: vector<u8>): ExternalResult<T> {
        ExternalResult::err_reason(v)
    }

    fun pack_panic_code<T>(v: U256): ExternalResult<T> {
        ExternalResult::panic(v)
    }

}
