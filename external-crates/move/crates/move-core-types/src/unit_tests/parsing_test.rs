use crate::parsing::{
    address::{NumericalAddress, ParsedAddress},
    types::{ParsedStructType, ParsedType},
    values::ParsedValue,
};
use crate::{account_address::AccountAddress, identifier::Identifier, u256::U256};
use proptest::prelude::*;
use proptest::proptest;

#[allow(clippy::unreadable_literal)]
#[test]
fn tests_parse_value_positive() {
    use ParsedValue as V;
    let cases: &[(&str, V)] = &[
        ("  0u8", V::U8(0)),
        ("0u8", V::U8(0)),
        ("0xF_Fu8", V::U8(255)),
        ("0xF__FF__Eu16", V::U16(u16::MAX - 1)),
        ("0xFFF_FF__FF_Cu32", V::U32(u32::MAX - 3)),
        ("255u8", V::U8(255)),
        ("255u256", V::U256(U256::from(255u64))),
        ("0", V::InferredNum(U256::from(0u64))),
        ("0123", V::InferredNum(U256::from(123u64))),
        ("0xFF", V::InferredNum(U256::from(0xFFu64))),
        ("0xF_F", V::InferredNum(U256::from(0xFFu64))),
        ("0xFF__", V::InferredNum(U256::from(0xFFu64))),
        (
            "0x12_34__ABCD_FF",
            V::InferredNum(U256::from(0x1234ABCDFFu64)),
        ),
        ("0u64", V::U64(0)),
        ("0x0u64", V::U64(0)),
        (
            "18446744073709551615",
            V::InferredNum(U256::from(18446744073709551615u128)),
        ),
        ("18446744073709551615u64", V::U64(18446744073709551615)),
        ("0u128", V::U128(0)),
        ("1_0u8", V::U8(1_0)),
        ("10_u8", V::U8(10)),
        ("1_000u64", V::U64(1_000)),
        ("1_000", V::InferredNum(U256::from(1_000u32))),
        ("1_0_0_0u64", V::U64(1_000)),
        ("1_000_000u128", V::U128(1_000_000)),
        (
            "340282366920938463463374607431768211455u128",
            V::U128(340282366920938463463374607431768211455),
        ),
        ("true", V::Bool(true)),
        ("false", V::Bool(false)),
        (
            "@0x0",
            V::Address(ParsedAddress::Numerical(NumericalAddress::new(
                AccountAddress::from_hex_literal("0x0")
                    .unwrap()
                    .into_bytes(),
                crate::parsing::parser::NumberFormat::Hex,
            ))),
        ),
        (
            "@0",
            V::Address(ParsedAddress::Numerical(NumericalAddress::new(
                AccountAddress::from_hex_literal("0x0")
                    .unwrap()
                    .into_bytes(),
                crate::parsing::parser::NumberFormat::Hex,
            ))),
        ),
        (
            "@0x54afa3526",
            V::Address(ParsedAddress::Numerical(NumericalAddress::new(
                AccountAddress::from_hex_literal("0x54afa3526")
                    .unwrap()
                    .into_bytes(),
                crate::parsing::parser::NumberFormat::Hex,
            ))),
        ),
        (
            "b\"hello\"",
            V::Vector("hello".as_bytes().iter().copied().map(V::U8).collect()),
        ),
        ("x\"7fff\"", V::Vector(vec![V::U8(0x7f), V::U8(0xff)])),
        ("x\"\"", V::Vector(vec![])),
        ("x\"00\"", V::Vector(vec![V::U8(0x00)])),
        (
            "x\"deadbeef\"",
            V::Vector(vec![V::U8(0xde), V::U8(0xad), V::U8(0xbe), V::U8(0xef)]),
        ),
    ];

    for (s, expected) in cases {
        assert_eq!(&ParsedValue::parse(s).unwrap(), expected)
    }
}

#[test]
fn tests_parse_value_negative() {
    /// Test cases for the parser that should always fail.
    const PARSE_VALUE_NEGATIVE_TEST_CASES: &[&str] = &[
            "-3",
            "0u42",
            "0u645",
            "0u64x",
            "0u6 4",
            "0u",
            "_10",
            "_10_u8",
            "_10__u8",
            "10_u8__",
            "0xFF_u8_",
            "0xF_u8__",
            "0x_F_u8__",
            "_",
            "__",
            "__4",
            "_u8",
            "5_bool",
            "256u8",
            "4294967296u32",
            "65536u16",
            "18446744073709551616u64",
            "340282366920938463463374607431768211456u128",
            "340282366920938463463374607431768211456340282366920938463463374607431768211456340282366920938463463374607431768211456340282366920938463463374607431768211456u256",
            "0xg",
            "0x00g0",
            "0x",
            "0x_",
            "",
            "@@",
            "()",
            "x\"ffff",
            "x\"a \"",
            "x\" \"",
            "x\"0g\"",
            "x\"0\"",
            "garbage",
            "true3",
            "3false",
            "3 false",
            "",
            "0XFF",
            "0X0",
        ];

    for s in PARSE_VALUE_NEGATIVE_TEST_CASES {
        assert!(
            ParsedValue::<()>::parse(s).is_err(),
            "Unexpectedly succeeded in parsing: {}",
            s
        )
    }
}

#[test]
fn test_parse_type_negative() {
    for s in &[
        "_",
        "_::_::_",
        "0x1::_",
        "0x1::__::_",
        "0x1::_::__",
        "0x1::_::foo",
        "0x1::foo::_",
        "0x1::_::_",
        "0x1::bar::foo<0x1::_::foo>",
    ] {
        assert!(
            ParsedType::parse(s).is_err(),
            "Parsed type {s} but should have failed"
        );
    }
}

#[test]
fn test_parse_struct_negative() {
    for s in &[
        "_",
        "_::_::_",
        "0x1::_",
        "0x1::__::_",
        "0x1::_::__",
        "0x1::_::foo",
        "0x1::foo::_",
        "0x1::_::_",
        "0x1::bar::foo<0x1::_::foo>",
    ] {
        assert!(
            ParsedStructType::parse(s).is_err(),
            "Parsed type {s} but should have failed"
        );
    }
}

#[test]
fn test_type_type() {
    for s in &[
        "u64",
        "bool",
        "vector<u8>",
        "vector<vector<u64>>",
        "address",
        "signer",
        "0x1::M::S",
        "0x2::M::S_",
        "0x3::M_::S",
        "0x4::M_::S_",
        "0x00000000004::M::S",
        "0x1::M::S<u64>",
        "0x1::M::S<0x2::P::Q>",
        "vector<0x1::M::S>",
        "vector<0x1::M_::S_>",
        "vector<vector<0x1::M_::S_>>",
        "0x1::M::S<vector<u8>>",
        "0x1::_bar::_BAR",
        "0x1::__::__",
        "0x1::_bar::_BAR<0x2::_____::______fooo______>",
        "0x1::__::__<0x2::_____::______fooo______, 0xff::Bar____::_______foo>",
    ] {
        assert!(ParsedType::parse(s).is_ok(), "Failed to parse type {}", s);
    }
}

#[test]
fn test_parse_valid_struct_type() {
    let valid = vec![
            "0x1::Foo::Foo",
            "0x1::Foo_Type::Foo",
            "0x1::Foo_::Foo",
            "0x1::X_123::X32_",
            "0x1::Foo::Foo_Type",
            "0x1::Foo::Foo<0x1::ABC::ABC>",
            "0x1::Foo::Foo<0x1::ABC::ABC_Type>",
            "0x1::Foo::Foo<u8>",
            "0x1::Foo::Foo<u16>",
            "0x1::Foo::Foo<u32>",
            "0x1::Foo::Foo<u64>",
            "0x1::Foo::Foo<u128>",
            "0x1::Foo::Foo<u256>",
            "0x1::Foo::Foo<bool>",
            "0x1::Foo::Foo<address>",
            "0x1::Foo::Foo<signer>",
            "0x1::Foo::Foo<vector<0x1::ABC::ABC>>",
            "0x1::Foo::Foo<u8,bool>",
            "0x1::Foo::Foo<u8,   bool>",
            "0x1::Foo::Foo<u8  ,bool>",
            "0x1::Foo::Foo<u8 , bool  ,    vector<u8>,address,signer>",
            "0x1::Foo::Foo<vector<0x1::Foo::Struct<0x1::XYZ::XYZ>>>",
            "0x1::Foo::Foo<0x1::Foo::Struct<vector<0x1::XYZ::XYZ>, 0x1::Foo::Foo<vector<0x1::Foo::Struct<0x1::XYZ::XYZ>>>>>",
            "0x1::_bar::_BAR",
            "0x1::__::__",
            "0x1::_bar::_BAR<0x2::_____::______fooo______>",
            "0x1::__::__<0x2::_____::______fooo______, 0xff::Bar____::_______foo>",
        ];
    for s in valid {
        assert!(
            ParsedStructType::parse(s).is_ok(),
            "Failed to parse struct {}",
            s
        );
    }
}

fn struct_type_gen() -> impl Strategy<Value = String> {
    (
        any::<AccountAddress>(),
        any::<Identifier>(),
        any::<Identifier>(),
    )
        .prop_map(|(address, module, name)| format!("0x{}::{}::{}", address, module, name))
}

proptest! {
    #[test]
    fn test_parse_valid_struct_type_proptest(s in struct_type_gen()) {
        prop_assert!(ParsedStructType::parse(&s).is_ok());
    }

    #[test]
    fn test_parse_valid_type_struct_only_proptest(s in struct_type_gen()) {
        prop_assert!(ParsedStructType::parse(&s).is_ok());
    }
}
