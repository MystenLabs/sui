#[evm_contract]
module 0x2::M {

    // Extra commas

    #[callable(sig=b"f(uint64,) ")]
    fun extra_comma_1(_x: u64) {

    }

    #[callable(sig=b"f(,uint64) ")]
    fun extra_comma_2(_x: u64) {

    }

    #[callable(sig=b"f() returns (uint64,) ")]
    fun extra_comma_3() : u64 {
        1
    }

    // Illegal chars

    #[callable(sig=b"f(()) ")]
    fun illegal_char_1() {
    }

    #[callable(sig=b"f() returns ([) ")]
    fun illegal_char_2() {
    }

    #[callable(sig=b"f() return () ")]
    fun illegal_char_3() {
    }

    #[callable(sig=b"f() returns uint64 ")]
    fun no_bracket() : u64 {
        0
    }

    // Illegal types

    #[callable(sig=b"f(uint9) returns (uint64) ")]
    fun illegal_int_1(_x: u8) : u64 {
        0
    }

    #[callable(sig=b"f(uint264) returns (uint64) ")]
    fun illegal_int_2(_x: u8) : u64 {
        0
    }

    #[callable(sig=b"f(int0) returns (uint64) ")]
    fun illegal_int_3(_x: u8) : u64 {
        0
    }

    #[callable(sig=b"f(fixed255x15) returns (uint64) ")]
    fun illegal_fixed_1(_x: u8) : u64 {
        0
    }

    #[callable(sig=b"f(ufixed256x81) returns (uint64) ")]
    fun illegal_fixed_2(_x: u8) : u64 {
        0
    }

    #[callable(sig=b"f(fix64x18) ")]
    fun illegal_fixed_3(_x: u128) {
    }

    #[callable(sig=b"f(ufixed64X18) ")]
    fun illegal_fixed_4(_x: u128) {
    }


    #[callable(sig=b"f(bytes0) returns (uint64) ")]
    fun illegal_bytes_1(_v: vector<u8>) : u64 {
        0
    }

    #[callable(sig=b"f(bytes33) returns (uint64) ")]
    fun illegal_bytes_2(_v: vector<u8>) : u64 {
        0
    }

    #[callable(sig=b"f(address pyable) returns (uint64) ")]
    fun illegal_address_1(_x: address) : u64 {
        0
    }

    #[callable(sig=b"f(addresspayable) returns (addresss) ")]
    fun illegal_address_2(x: address) : address {
        x
    }
}
