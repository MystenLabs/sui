#[evm_contract]
module 0x2::M {

    // Length difference

    #[callable(sig=b"f() returns ()")]
    fun len_typ_diff(x: u64): u64 {
        x
    }

    #[callable(sig=b"f() returns ()")]
    fun len_ret_diff(): u128 {
        0
    }

    // Primitive types

    #[callable(sig=b"f(int16)")]
    fun fun_u8(_x:u8) {
    }

    #[callable(sig=b"f(uint72)")]
    fun fun_u64(_x:u64) {
    }

    #[callable(sig=b"f() returns (int248)")]
    fun fun_u128(): u128 {
        0
    }

    #[callable(sig=b"f(bool)")]
    fun fun_u8_bool(_b: u8) {
    }

    #[callable(sig=b"f(address)")]
    fun fun_u128_address(_a: u128) {
    }

    #[callable(sig=b"f(uint160)")]
    fun fun_address_u160(_a: address) {
    }

    #[callable(sig=b"f(ufixed)")]
    fun fun_u128_ufixed(_a: u128) {
    }

    #[callable(sig=b"f(fixed128x18)")]
    fun fun_u128_fixed(_a: u128) {
    }

    // string

    #[callable(sig=b"f(string) returns (uint64)")]
    fun fun_vec_u128_str(_vec0: vector<u128>): u128 {
        2
    }

    // Dynamic bytes

    #[callable(sig=b"f(bytes) returns (uint64)")]
    fun fun_vec_u128_bytes(_vec0: vector<u128>): u128 {
        2
    }

    // Static bytes

    #[callable(sig=b"f(bytes32) returns (uint64)")]
    fun fun_vec_u64(_vec0: vector<u64>): u128 {
        2
    }

    // Static array

    #[callable(sig=b"f(int72[5]) returns (uint64)")]
    fun fun_vec_u64_int72_static(_vec0: vector<u64>): u128 {
        2
    }

    // Dynamic array

    #[callable(sig=b"f(int72[]) returns (uint64)")]
    fun fun_vec_u64_int72_dynamic(_vec0: vector<u64>): u128 {
        2
    }


}
