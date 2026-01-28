// Test: Constants and attributes
// EBNF: ConstantDecl, Attributes, Attribute, AttributeValue
module 0x42::constants_attrs;

const U8_MAX: u8 = 255;
const U64_MAX: u64 = 18446744073709551615;
const HEX_VALUE: u64 = 0xDEADBEEF;
const BOOL_TRUE: bool = true;
const BOOL_FALSE: bool = false;
const ADDR: address = @0x42;
const BYTE_STRING: vector<u8> = b"hello";
const HEX_STRING: vector<u8> = x"CAFE";

#[test]
fun test_constants() {
    assert!(U8_MAX == 255, 0);
}

#[test, expected_failure(abort_code = 1)]
fun test_abort() {
    abort 1
}

#[allow(unused_variable)]
fun with_allow() {
    let x: u64 = 1;
}

#[error]
const EInvalidValue: u64 = 0;

#[test_only]
public struct TestOnly has drop { value: u64 }
