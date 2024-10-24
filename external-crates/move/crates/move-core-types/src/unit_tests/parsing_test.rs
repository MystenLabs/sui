use crate::{
    account_address::AccountAddress,
    identifier::Identifier,
    language_storage::{ModuleId, StructTag, TypeTag},
    parsing::{
        address::{NumericalAddress, ParsedAddress},
        parser::parse,
        types::{ParsedFqName, ParsedType, TypeToken},
        values::ParsedValue,
    },
    u256::U256,
};
use anyhow::bail;
use num::BigUint;
use proptest::{prelude::*, proptest};
use std::str::FromStr;

const VALID_ADDRS: &[&str] = &[
    "0x0",
    "0x1",
    "1",
    "123",
    "0x123",
    "0x1234567890abcdef",
    "100_00_00",
    "0x0_0_0_0",
    "0x0000000000000000000000000000000000000000000000000000000000000000",
    "0x0_00000_0000000000000000000000000000000000000000000000000_000000000",
    "000000000000000000000000000000000000000000000000000000000000000000000000000000",
    "00_0000000000000000000000000000000000000000000000000000000_00000000000000000_0000",
];

const INVALID_ADDRS: &[&str] = &[
    "_x",
    "0x",
    "_0x0",
    "_0",
    "0x_",
    "0x_00",
    "+0x0",
    "+0",
    "0xg",
    "0x0g",
    "0X0",
    "_0x0",
    "_0x0_",
    "_0",
    "_0_",
    "_00_",
    "_0_0_",
    "0x_00",
    "0x00000000000000000000000000000000000000000000000000000000000000000",
    "0x0000000000000000000000000000000000000000000000000000000000_0000000",
    "0x_0_00000_0000000000000000000000000000000000000000000000000_000000000",
    "0000000000000000000000000000000000000000000000000000000000000000000000000000000",
    "000_0000000000000000000000000000000000000000000000000000000_00000000000000000_0000",
];

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
        "0x1::bar::bar::foo",
        "0x1::Foo::Foo<",
        "0x1::Foo::Foo<0x1::ABC::ABC",
        "0x1::Foo::Foo<0x1::ABC::ABC::>",
        "0x1::Foo::Foo<0x1::ABC::ABC::A>",
        "0x1::Foo::Foo<>",
        "0x1::Foo::Foo<,>",
        "0x1::Foo::Foo<,",
        "0x1::Foo::Foo,>",
        "0x1::Foo::Foo>",
        "0x1::Foo::Foo,",
        "_0x0_0::a::a",
        "_0x_00::a::a",
        "_0_0::a::a",
    ] {
        assert!(
            TypeTag::from_str(s).is_err(),
            "Parsed type {s} but should have failed"
        );
    }
}

#[test]
fn test_type_type() {
    for s in &[
        "u8",
        "u16",
        "u32",
        "u64",
        "u128",
        "u256",
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
        "0x0_0::a::a",
        "0_0::a::a",
    ] {
        assert!(TypeTag::from_str(s).is_ok(), "Failed to parse type {}", s);
    }

    for valid_addr in VALID_ADDRS {
        assert!(
            TypeTag::from_str(&format!("{valid_addr}::a::a")).is_ok(),
            "Failed to parse type {}::a::a",
            valid_addr
        );
    }

    for invalid_addr in INVALID_ADDRS {
        assert!(
            TypeTag::from_str(&format!("{invalid_addr}::a::a")).is_err(),
            "Parse type {}::a::a but should have failed",
            invalid_addr
        );
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
            StructTag::from_str(s).is_ok(),
            "Failed to parse struct {}",
            s
        );
    }
}

#[test]
fn test_parse_type_list() {
    let valid_with_trails = &[
        "<u64,>",
        "<u64, 0x0::a::a,>",
        "<u64, 0x0::a::a, 0x0::a::a<0x0::a::a>,>",
    ];
    let valid_no_trails = &[
        "<u64>",
        "<u64, 0x0::a::a>",
        "<u64, 0x0::a::a, 0x0::a::a<0x0::a::a>>",
    ];
    let invalid = &[
        "<>",
        "<,>",
        "<u64,,>",
        "<,u64>",
        "<,u64,>",
        ",",
        "",
        "<",
        "<<",
        "><",
        ">,<",
        ">,",
        ",>",
        ",,",
        ">>",
        "<u64, u64",
        "<u64, u64,",
        "u64>",
        "u64,>",
        "u64, u64,>",
        "u64, u64,",
        "u64, u64",
        "u64 u64",
        "<u64 u64>",
        "<u64 u64,>",
        "u64 u64,",
        "<u64, 0x0::a::a, 0x0::a::a<0x::a::a>",
        "<u64, 0x0::a::a, 0x0::a::a<0x0::a::a>,",
        "<u64, 0x0::a::a, 0x0::a::a<0x0::a::a>,,>",
    ];

    for t in valid_no_trails.iter().chain(valid_with_trails.iter()) {
        assert!(parse_type_tags(t, true).is_ok());
    }

    for t in valid_no_trails {
        assert!(parse_type_tags(t, false).is_ok());
    }

    for t in valid_with_trails {
        assert!(parse_type_tags(t, false).is_err());
    }

    for t in invalid {
        assert!(parse_type_tags(t, true).is_err(), "parsed type {}", t);
        assert!(parse_type_tags(t, false).is_err(), "parsed type {}", t);
    }
}

fn struct_type_gen0() -> impl Strategy<Value = String> {
    (
        any::<AccountAddress>(),
        any::<Identifier>(),
        any::<Identifier>(),
    )
        .prop_map(|(address, module, name)| format!("0x{}::{}::{}", address, module, name))
}

fn struct_type_gen1() -> impl Strategy<Value = String> {
    (any::<U256>(), any::<Identifier>(), any::<Identifier>())
        .prop_map(|(address, module, name)| format!("{}::{}::{}", address, module, name))
}

fn module_id_gen0() -> impl Strategy<Value = String> {
    (any::<AccountAddress>(), any::<Identifier>())
        .prop_map(|(address, module)| format!("0x{address}::{module}"))
}

fn module_id_gen1() -> impl Strategy<Value = String> {
    (any::<U256>(), any::<Identifier>())
        .prop_map(|(address, module)| format!("{address}::{module}"))
}

fn fq_id_gen0() -> impl Strategy<Value = String> {
    (
        any::<AccountAddress>(),
        any::<Identifier>(),
        any::<Identifier>(),
    )
        .prop_map(|(address, module, name)| format!("0x{address}::{module}::{name}"))
}

fn fq_id_gen1() -> impl Strategy<Value = String> {
    (any::<U256>(), any::<Identifier>(), any::<Identifier>())
        .prop_map(|(address, module, name)| format!("{address}::{module}::{name}"))
}

fn parse_type_tags(s: &str, allow_trailing_delim: bool) -> anyhow::Result<Vec<ParsedType>> {
    parse(s, |parser| {
        parser.advance(TypeToken::Lt)?;
        let parsed = parser.parse_list(
            |parser| parser.parse_type(),
            TypeToken::Comma,
            TypeToken::Gt,
            allow_trailing_delim,
        )?;
        parser.advance(TypeToken::Gt)?;
        if parsed.is_empty() {
            bail!("expected at least one type argument")
        }
        Ok(parsed)
    })
}

#[test]
fn address_parsing() {
    for valid_addr in VALID_ADDRS {
        assert!(
            ParsedAddress::parse(valid_addr).is_ok(),
            "parsed address {}",
            valid_addr
        );
    }

    for invalid_addr in INVALID_ADDRS {
        assert!(ParsedAddress::parse(invalid_addr).is_err());
    }
}

proptest! {
    #[test]
    fn parse_type_tag_list(t in struct_type_gen0(), args in proptest::collection::vec(struct_type_gen0(), 1..=100)) {
        let s_no_trail = format!("<{}>", args.join(","));
        let s_with_trail = format!("<{},>", args.join(","));
        let s_no_trail_no_trail = parse_type_tags(&s_no_trail, false);
        let s_no_trail_allow_trail = parse_type_tags(&s_no_trail, true);
        let s_with_trail_no_trail = parse_type_tags(&s_with_trail, false);
        let s_with_trail_allow_trail = parse_type_tags(&s_with_trail, true);
        prop_assert!(s_no_trail_no_trail.is_ok());
        prop_assert!(s_no_trail_allow_trail.is_ok());
        prop_assert!(s_with_trail_no_trail.is_err());
        prop_assert!(s_with_trail_allow_trail.is_ok());
        let t_with_trail = format!("{t}{s_no_trail}");
        let t_no_trail = format!("{t}{s_with_trail}");
        let t_with_trail = TypeTag::from_str(&t_with_trail);
        let t_no_trail = TypeTag::from_str(&t_no_trail);
        prop_assert!(t_with_trail.is_ok());
        prop_assert!(t_no_trail.is_ok());
        prop_assert_eq!(t_with_trail.unwrap(), t_no_trail.unwrap());
    }

    #[test]
    fn test_parse_valid_struct_type_proptest0(s in struct_type_gen0(), x in r#"(::foo)[^a-zA-Z0-9_\s]+"#) {
        prop_assert!(StructTag::from_str(&s).is_ok());
        prop_assert!(TypeTag::from_str(&s).is_ok());
        prop_assert!(ParsedFqName::parse(&s).is_ok());
        prop_assert!(ModuleId::from_str(&s).is_err());
        prop_assert!(ParsedAddress::parse(&s).is_err());

        // Add remainder string
        let s = s + &x;
        prop_assert!(StructTag::from_str(&s).is_err());
        prop_assert!(TypeTag::from_str(&s).is_err());
        prop_assert!(ParsedFqName::parse(&s).is_err());
        prop_assert!(ModuleId::from_str(&s).is_err());
        prop_assert!(ParsedAddress::parse(&s).is_err());

    }

    #[test]
    fn test_parse_valid_struct_type_proptest1(s in struct_type_gen1(), x in r#"(::foo)[^a-zA-Z0-9_\s]+"#) {
        prop_assert!(StructTag::from_str(&s).is_ok());
        prop_assert!(TypeTag::from_str(&s).is_ok());
        prop_assert!(ParsedFqName::parse(&s).is_ok());
        prop_assert!(ModuleId::from_str(&s).is_err());
        prop_assert!(ParsedAddress::parse(&s).is_err());
        // add remainder string
        let s = s + &x;
        prop_assert!(StructTag::from_str(&s).is_err());
        prop_assert!(TypeTag::from_str(&s).is_err());
        prop_assert!(ParsedFqName::parse(&s).is_err());
        prop_assert!(ModuleId::from_str(&s).is_err());
        prop_assert!(ParsedAddress::parse(&s).is_err());
    }

    #[test]
    fn test_parse_valid_module_id_proptest0(s in module_id_gen0(), x in r#"[^a-zA-Z0-9_\s]+"#) {
        prop_assert!(ModuleId::from_str(&s).is_ok());
        prop_assert!(StructTag::from_str(&s).is_err());
        prop_assert!(TypeTag::from_str(&s).is_err());
        prop_assert!(ParsedFqName::parse(&s).is_err());
        prop_assert!(ParsedAddress::parse(&s).is_err());
        // add remainder string
        let s = s + &x;
        prop_assert!(ModuleId::from_str(&s).is_err());
        prop_assert!(StructTag::from_str(&s).is_err());
        prop_assert!(TypeTag::from_str(&s).is_err());
        prop_assert!(ParsedFqName::parse(&s).is_err());
        prop_assert!(ParsedAddress::parse(&s).is_err());
    }

    #[test]
    fn test_parse_valid_module_id_proptest1(s in module_id_gen1(), x in r#"[^a-zA-Z0-9_\s]+"#) {
        prop_assert!(ModuleId::from_str(&s).is_ok());
        prop_assert!(StructTag::from_str(&s).is_err());
        prop_assert!(TypeTag::from_str(&s).is_err());
        prop_assert!(ParsedFqName::parse(&s).is_err());
        prop_assert!(ParsedAddress::parse(&s).is_err());
        // add remainder String
        let s = s + &x;
        prop_assert!(ModuleId::from_str(&s).is_err());
        prop_assert!(StructTag::from_str(&s).is_err());
        prop_assert!(TypeTag::from_str(&s).is_err());
        prop_assert!(ParsedFqName::parse(&s).is_err());
        prop_assert!(ParsedAddress::parse(&s).is_err());

    }

    #[test]
    fn test_parse_valid_fq_id_proptest0(s in fq_id_gen0(), x in r#"[^a-zA-Z0-9_\s]+"#) {
        prop_assert!(ParsedFqName::parse(&s).is_ok());
        prop_assert!(StructTag::from_str(&s).is_ok());
        prop_assert!(TypeTag::from_str(&s).is_ok());
        prop_assert!(ModuleId::from_str(&s).is_err());
        prop_assert!(ParsedAddress::parse(&s).is_err());
        // add remainder string
        let s = s + &x;
        prop_assert!(ParsedFqName::parse(&s).is_err());
        prop_assert!(StructTag::from_str(&s).is_err());
        prop_assert!(TypeTag::from_str(&s).is_err());
        prop_assert!(ModuleId::from_str(&s).is_err());
        prop_assert!(ParsedAddress::parse(&s).is_err());
    }

    #[test]
    fn test_parse_valid_fq_id_proptest1(s in fq_id_gen1(), x in r#"[^a-zA-Z0-9_\s]+"#) {
        prop_assert!(ParsedFqName::parse(&s).is_ok());
        prop_assert!(StructTag::from_str(&s).is_ok());
        prop_assert!(TypeTag::from_str(&s).is_ok());
        prop_assert!(ModuleId::from_str(&s).is_err());
        prop_assert!(ParsedAddress::parse(&s).is_err());
        let s = s + &x;
        prop_assert!(ParsedFqName::parse(&s).is_err());
        prop_assert!(StructTag::from_str(&s).is_err());
        prop_assert!(TypeTag::from_str(&s).is_err());
        prop_assert!(ModuleId::from_str(&s).is_err());
        prop_assert!(ParsedAddress::parse(&s).is_err());
    }

    #[test]
    fn test_parse_valid_numeric_address(s in "[0-9]{64}", x in r#"[^a-zA-Z0-9_\s]+"#) {
        prop_assert!(AccountAddress::from_str(&s).is_ok());
        prop_assert!(ParsedAddress::parse(&s).is_ok());
        prop_assert!(ParsedFqName::parse(&s).is_err());
        prop_assert!(ModuleId::from_str(&s).is_err());
        prop_assert!(StructTag::from_str(&s).is_err());
        prop_assert!(TypeTag::from_str(&s).is_err());
        // add remainder string
        let s = s + &x;
        prop_assert!(AccountAddress::from_str(&s).is_err());
        prop_assert!(ParsedAddress::parse(&s).is_err());
        prop_assert!(ParsedFqName::parse(&s).is_err());
        prop_assert!(ModuleId::from_str(&s).is_err());
        prop_assert!(StructTag::from_str(&s).is_err());
        prop_assert!(TypeTag::from_str(&s).is_err());
    }

    #[test]
    fn decimal_parse_parity(s in "[0-9]{64}") {
        let bigint_parsed = {
            let bytes = BigUint::parse_bytes(s.as_bytes(), 10).unwrap().to_bytes_be();
            let mut result = [0u8; AccountAddress::LENGTH];
            result[(AccountAddress::LENGTH - bytes.len())..].clone_from_slice(&bytes);
            result
        };
        let u256_parsed = U256::from_str(&s).unwrap();
        prop_assert_eq!(bigint_parsed, u256_parsed.to_be_bytes(), "Parsed addresses do not match: {}", s);
    }

    #[test]
    fn hex_parse_parity(s in "0x[0-9a-fA-F]{1,64}") {
        let bigint_parsed = {
            let bytes = BigUint::parse_bytes(s[2..].as_bytes(), 16).unwrap().to_bytes_be();
            let mut result = [0u8; AccountAddress::LENGTH];
            result[(AccountAddress::LENGTH - bytes.len())..].clone_from_slice(&bytes);
            result
        };
        let addr_parsed = AccountAddress::from_hex_literal(&s).unwrap().into_bytes();
        let u256_parsed = AccountAddress::new(U256::from_str_radix(&s[2..], 16).unwrap().to_be_bytes()).into_bytes();
        prop_assert_eq!(bigint_parsed, addr_parsed, "Parsed addresses do not match: {}", s);
        prop_assert_eq!(addr_parsed, u256_parsed, "Parsed addresses do not match: {}", s);
    }

    #[test]
    fn test_parse_different_length_numeric_addresses(s in "[0-9]{1,63}", x in r#"[^a-zA-Z0-9_\s]+"#) {
        prop_assert!(AccountAddress::from_str(&s).is_err());
        prop_assert!(ParsedAddress::parse(&s).is_ok());
        prop_assert!(ParsedFqName::parse(&s).is_err());
        prop_assert!(ModuleId::from_str(&s).is_err());
        prop_assert!(StructTag::from_str(&s).is_err());
        prop_assert!(TypeTag::from_str(&s).is_err());
        // add remainder string
        let s = s + &x;
        prop_assert!(AccountAddress::from_str(&s).is_err());
        prop_assert!(ParsedAddress::parse(&s).is_err());
        prop_assert!(ParsedFqName::parse(&s).is_err());
        prop_assert!(ModuleId::from_str(&s).is_err());
        prop_assert!(StructTag::from_str(&s).is_err());
        prop_assert!(TypeTag::from_str(&s).is_err());
    }

    #[test]
    fn test_parse_valid_hex_address(s in "0x[0-9a-fA-F]{64}", x in r#"[^a-zA-Z0-9_\s]+"#) {
        prop_assert!(AccountAddress::from_str(&s).is_ok());
        prop_assert!(ParsedAddress::parse(&s).is_ok());
        prop_assert!(ParsedFqName::parse(&s).is_err());
        prop_assert!(ModuleId::from_str(&s).is_err());
        prop_assert!(StructTag::from_str(&s).is_err());
        prop_assert!(TypeTag::from_str(&s).is_err());
        // add remainder string
        let s = s + &x;
        prop_assert!(AccountAddress::from_str(&s).is_err());
        prop_assert!(ParsedAddress::parse(&s).is_err());
        prop_assert!(ParsedFqName::parse(&s).is_err());
        prop_assert!(ModuleId::from_str(&s).is_err());
        prop_assert!(StructTag::from_str(&s).is_err());
        prop_assert!(TypeTag::from_str(&s).is_err());
    }

    #[test]
    fn test_parse_invalid_hex_address(s in "[0-9]{63}[a-fA-F]{1}", x in r#"[^a-zA-Z0-9_\s]+"#) {
        prop_assert!(AccountAddress::from_str(&s).is_ok());
        prop_assert!(ParsedAddress::parse(&s).is_err());
        prop_assert!(ParsedFqName::parse(&s).is_err());
        prop_assert!(ModuleId::from_str(&s).is_err());
        prop_assert!(StructTag::from_str(&s).is_err());
        prop_assert!(TypeTag::from_str(&s).is_err());
        // add remainder string
        let s = s + &x;
        prop_assert!(AccountAddress::from_str(&s).is_err());
        prop_assert!(ParsedAddress::parse(&s).is_err());
        prop_assert!(ParsedFqName::parse(&s).is_err());
        prop_assert!(ModuleId::from_str(&s).is_err());
        prop_assert!(StructTag::from_str(&s).is_err());
        prop_assert!(TypeTag::from_str(&s).is_err());
    }
}
