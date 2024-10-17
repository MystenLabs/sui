// Copyright (c) The Move Contributors
// SPDX-License-Identifier: Apache-2.0

use std::str::FromStr;

use crate::{
    account_address::AccountAddress,
    annotated_extractor::{Element as E, Extractor},
    annotated_value::{MoveTypeLayout, MoveValue},
    language_storage::TypeTag,
    unit_tests::visitor_test::{
        enum_layout_, serialize, struct_layout_, struct_value_, variant_value_, PrintVisitor,
    },
};

#[test]
fn struct_() {
    let expect = r#"
[0] struct 0x0::foo::Bar {
    a: u8,
    b: u16,
    c: u32,
    d: u64,
    e: u128,
    f: u256,
    g: bool,
    h: address,
    i: signer,
    j: vector<u8>,
    k: struct 0x0::foo::Baz {
        l: u8,
    },
    m: enum 0x0::foo::Qux {
        n {
            o: u8,
        },
    },
    p: vector<struct 0x0::foo::Quy {
        q: u8,
        r: bool,
    }>,
}
[1] 1: u8
[1] 2: u16
[1] 3: u32
[1] 4: u64
[1] 5: u128
[1] 6: u256
[1] true: bool
[1] 0000000000000000000000000000000000000000000000000000000000000000: address
[1] 0000000000000000000000000000000000000000000000000000000000000000: signer
[1] vector<u8>
[2] 7: u8
[2] 8: u8
[2] 9: u8
[1] struct 0x0::foo::Baz {
    l: u8,
}
[2] 10: u8
[1] enum 0x0::foo::Qux {
    n {
        o: u8,
    },
}
[2] 11: u8
[1] vector<struct 0x0::foo::Quy {
    q: u8,
    r: bool,
}>
[2] struct 0x0::foo::Quy {
    q: u8,
    r: bool,
}
[3] 12: u8
[3] true: bool
[2] struct 0x0::foo::Quy {
    q: u8,
    r: bool,
}
[3] 13: u8
[3] false: bool
    "#;

    for path in enumerate_paths(vec![C::Opt(E::Type(&type_("0x0::foo::Bar")))]) {
        assert_path(test_struct(), path, expect);
    }
}

#[test]
fn struct_a() {
    let expect = r#"
[0] 1: u8
    "#;

    for path in enumerate_paths(vec![
        C::Opt(E::Type(&type_("0x0::foo::Bar"))),
        C::Req(vec![E::Field("a"), E::Index(0)]),
        C::Opt(E::Type(&type_("u8"))),
    ]) {
        assert_path(test_struct(), path, expect);
    }
}

#[test]
fn struct_b() {
    let expect = r#"
[0] 2: u16
    "#;

    for path in enumerate_paths(vec![
        C::Opt(E::Type(&type_("0x0::foo::Bar"))),
        C::Req(vec![E::Field("b"), E::Index(1)]),
        C::Opt(E::Type(&type_("u16"))),
    ]) {
        assert_path(test_struct(), path, expect);
    }
}

#[test]
fn struct_c() {
    let expect = r#"
[0] 3: u32
    "#;

    for path in enumerate_paths(vec![
        C::Opt(E::Type(&type_("0x0::foo::Bar"))),
        C::Req(vec![E::Field("c"), E::Index(2)]),
        C::Opt(E::Type(&type_("u32"))),
    ]) {
        assert_path(test_struct(), path, expect);
    }
}

#[test]
fn struct_d() {
    let expect = r#"
[0] 4: u64
    "#;

    for path in enumerate_paths(vec![
        C::Opt(E::Type(&type_("0x0::foo::Bar"))),
        C::Req(vec![E::Field("d"), E::Index(3)]),
        C::Opt(E::Type(&type_("u64"))),
    ]) {
        assert_path(test_struct(), path, expect);
    }
}

#[test]
fn struct_e() {
    let expect = r#"
[0] 5: u128
    "#;

    for path in enumerate_paths(vec![
        C::Opt(E::Type(&type_("0x0::foo::Bar"))),
        C::Req(vec![E::Field("e"), E::Index(4)]),
        C::Opt(E::Type(&type_("u128"))),
    ]) {
        assert_path(test_struct(), path, expect);
    }
}

#[test]
fn struct_f() {
    let expect = r#"
[0] 6: u256
    "#;

    for path in enumerate_paths(vec![
        C::Opt(E::Type(&type_("0x0::foo::Bar"))),
        C::Req(vec![E::Field("f"), E::Index(5)]),
        C::Opt(E::Type(&type_("u256"))),
    ]) {
        assert_path(test_struct(), path, expect);
    }
}

#[test]
fn struct_g() {
    let expect = r#"
[0] true: bool
    "#;

    for path in enumerate_paths(vec![
        C::Opt(E::Type(&type_("0x0::foo::Bar"))),
        C::Req(vec![E::Field("g"), E::Index(6)]),
        C::Opt(E::Type(&type_("bool"))),
    ]) {
        assert_path(test_struct(), path, expect);
    }
}

#[test]
fn struct_h() {
    let expect = r#"
[0] 0000000000000000000000000000000000000000000000000000000000000000: address
    "#;

    for path in enumerate_paths(vec![
        C::Opt(E::Type(&type_("0x0::foo::Bar"))),
        C::Req(vec![E::Field("h"), E::Index(7)]),
        C::Opt(E::Type(&type_("address"))),
    ]) {
        assert_path(test_struct(), path, expect);
    }
}

#[test]
fn struct_i() {
    let expect = r#"
[0] 0000000000000000000000000000000000000000000000000000000000000000: signer
    "#;

    for path in enumerate_paths(vec![
        C::Opt(E::Type(&type_("0x0::foo::Bar"))),
        C::Req(vec![E::Field("i"), E::Index(8)]),
        C::Opt(E::Type(&type_("signer"))),
    ]) {
        assert_path(test_struct(), path, expect);
    }
}

#[test]
fn struct_j() {
    let expect = r#"
[0] vector<u8>
[1] 7: u8
[1] 8: u8
[1] 9: u8
    "#;

    for path in enumerate_paths(vec![
        C::Opt(E::Type(&type_("0x0::foo::Bar"))),
        C::Req(vec![E::Field("j"), E::Index(9)]),
        C::Opt(E::Type(&type_("vector<u8>"))),
    ]) {
        assert_path(test_struct(), path, expect);
    }
}

#[test]
fn struct_j_0() {
    let expect = r#"
[0] 7: u8
    "#;

    for path in enumerate_paths(vec![
        C::Opt(E::Type(&type_("0x0::foo::Bar"))),
        C::Req(vec![E::Field("j"), E::Index(9)]),
        C::Opt(E::Type(&type_("vector<u8>"))),
        C::Req(vec![E::Index(0)]),
        C::Opt(E::Type(&type_("u8"))),
    ]) {
        assert_path(test_struct(), path, expect);
    }
}

#[test]
fn struct_j_1() {
    let expect = r#"
[0] 8: u8
    "#;

    for path in enumerate_paths(vec![
        C::Opt(E::Type(&type_("0x0::foo::Bar"))),
        C::Req(vec![E::Field("j"), E::Index(9)]),
        C::Opt(E::Type(&type_("vector<u8>"))),
        C::Req(vec![E::Index(1)]),
        C::Opt(E::Type(&type_("u8"))),
    ]) {
        assert_path(test_struct(), path, expect);
    }
}

#[test]
fn struct_j_2() {
    let expect = r#"
[0] 9: u8
    "#;

    for path in enumerate_paths(vec![
        C::Opt(E::Type(&type_("0x0::foo::Bar"))),
        C::Req(vec![E::Field("j"), E::Index(9)]),
        C::Opt(E::Type(&type_("vector<u8>"))),
        C::Req(vec![E::Index(2)]),
        C::Opt(E::Type(&type_("u8"))),
    ]) {
        assert_path(test_struct(), path, expect);
    }
}

#[test]
fn struct_k() {
    let expect = r#"
[0] struct 0x0::foo::Baz {
    l: u8,
}
[1] 10: u8
    "#;

    for path in enumerate_paths(vec![
        C::Opt(E::Type(&type_("0x0::foo::Bar"))),
        C::Req(vec![E::Field("k"), E::Index(10)]),
        C::Opt(E::Type(&type_("0x0::foo::Baz"))),
    ]) {
        assert_path(test_struct(), path, expect);
    }
}

#[test]
fn struct_k_l() {
    let expect = r#"
[0] 10: u8
    "#;

    for path in enumerate_paths(vec![
        C::Opt(E::Type(&type_("0x0::foo::Bar"))),
        C::Req(vec![E::Field("k"), E::Index(10)]),
        C::Opt(E::Type(&type_("0x0::foo::Baz"))),
        C::Req(vec![E::Field("l"), E::Index(0)]),
        C::Opt(E::Type(&type_("u8"))),
    ]) {
        assert_path(test_struct(), path, expect);
    }
}

#[test]
fn struct_m() {
    let expect = r#"
[0] enum 0x0::foo::Qux {
    n {
        o: u8,
    },
}
[1] 11: u8
    "#;

    for path in enumerate_paths(vec![
        C::Opt(E::Type(&type_("0x0::foo::Bar"))),
        C::Req(vec![E::Field("m"), E::Index(11)]),
        C::Opt(E::Type(&type_("0x0::foo::Qux"))),
        C::Opt(E::Variant("n")),
    ]) {
        assert_path(test_struct(), path, expect);
    }
}

#[test]
fn struct_m_o() {
    let expect = r#"
[0] 11: u8
    "#;

    for path in enumerate_paths(vec![
        C::Opt(E::Type(&type_("0x0::foo::Bar"))),
        C::Req(vec![E::Field("m"), E::Index(11)]),
        C::Opt(E::Type(&type_("0x0::foo::Qux"))),
        C::Opt(E::Variant("n")),
        C::Req(vec![E::Field("o"), E::Index(0)]),
        C::Opt(E::Type(&type_("u8"))),
    ]) {
        assert_path(test_struct(), path, expect);
    }
}

#[test]
fn struct_p() {
    let expect = r#"
[0] vector<struct 0x0::foo::Quy {
    q: u8,
    r: bool,
}>
[1] struct 0x0::foo::Quy {
    q: u8,
    r: bool,
}
[2] 12: u8
[2] true: bool
[1] struct 0x0::foo::Quy {
    q: u8,
    r: bool,
}
[2] 13: u8
[2] false: bool
    "#;

    for path in enumerate_paths(vec![
        C::Opt(E::Type(&type_("0x0::foo::Bar"))),
        C::Req(vec![E::Field("p"), E::Index(12)]),
        C::Opt(E::Type(&type_("vector<0x0::foo::Quy>"))),
    ]) {
        assert_path(test_struct(), path, expect);
    }
}

#[test]
fn struct_p_0() {
    let expect = r#"
[0] struct 0x0::foo::Quy {
    q: u8,
    r: bool,
}
[1] 12: u8
[1] true: bool
    "#;

    for path in enumerate_paths(vec![
        C::Opt(E::Type(&type_("0x0::foo::Bar"))),
        C::Req(vec![E::Field("p"), E::Index(12)]),
        C::Opt(E::Type(&type_("vector<0x0::foo::Quy>"))),
        C::Req(vec![E::Index(0)]),
        C::Opt(E::Type(&type_("0x0::foo::Quy"))),
    ]) {
        assert_path(test_struct(), path, expect);
    }
}

#[test]
fn struct_p_0_q() {
    let expect = r#"
[0] 12: u8
    "#;

    for path in enumerate_paths(vec![
        C::Opt(E::Type(&type_("0x0::foo::Bar"))),
        C::Req(vec![E::Field("p"), E::Index(12)]),
        C::Opt(E::Type(&type_("vector<0x0::foo::Quy>"))),
        C::Req(vec![E::Index(0)]),
        C::Opt(E::Type(&type_("0x0::foo::Quy"))),
        C::Req(vec![E::Field("q"), E::Index(0)]),
        C::Opt(E::Type(&type_("u8"))),
    ]) {
        assert_path(test_struct(), path, expect);
    }
}

#[test]
fn struct_p_0_r() {
    let expect = r#"
[0] true: bool
    "#;

    for path in enumerate_paths(vec![
        C::Opt(E::Type(&type_("0x0::foo::Bar"))),
        C::Req(vec![E::Field("p"), E::Index(12)]),
        C::Opt(E::Type(&type_("vector<0x0::foo::Quy>"))),
        C::Req(vec![E::Index(0)]),
        C::Opt(E::Type(&type_("0x0::foo::Quy"))),
        C::Req(vec![E::Field("r"), E::Index(1)]),
        C::Opt(E::Type(&type_("bool"))),
    ]) {
        assert_path(test_struct(), path, expect);
    }
}

#[test]
fn struct_p_1() {
    let expect = r#"
[0] struct 0x0::foo::Quy {
    q: u8,
    r: bool,
}
[1] 13: u8
[1] false: bool
    "#;

    for path in enumerate_paths(vec![
        C::Opt(E::Type(&type_("0x0::foo::Bar"))),
        C::Req(vec![E::Field("p"), E::Index(12)]),
        C::Opt(E::Type(&type_("vector<0x0::foo::Quy>"))),
        C::Req(vec![E::Index(1)]),
        C::Opt(E::Type(&type_("0x0::foo::Quy"))),
    ]) {
        assert_path(test_struct(), path, expect);
    }
}

#[test]
fn struct_p_1_q() {
    let expect = r#"
[0] 13: u8
    "#;

    for path in enumerate_paths(vec![
        C::Opt(E::Type(&type_("0x0::foo::Bar"))),
        C::Req(vec![E::Field("p"), E::Index(12)]),
        C::Opt(E::Type(&type_("vector<0x0::foo::Quy>"))),
        C::Req(vec![E::Index(1)]),
        C::Opt(E::Type(&type_("0x0::foo::Quy"))),
        C::Req(vec![E::Field("q"), E::Index(0)]),
        C::Opt(E::Type(&type_("u8"))),
    ]) {
        assert_path(test_struct(), path, expect);
    }
}

#[test]
fn struct_p_1_r() {
    let expect = r#"
[0] false: bool
    "#;

    for path in enumerate_paths(vec![
        C::Opt(E::Type(&type_("0x0::foo::Bar"))),
        C::Req(vec![E::Field("p"), E::Index(12)]),
        C::Opt(E::Type(&type_("vector<0x0::foo::Quy>"))),
        C::Req(vec![E::Index(1)]),
        C::Opt(E::Type(&type_("0x0::foo::Quy"))),
        C::Req(vec![E::Field("r"), E::Index(1)]),
        C::Opt(E::Type(&type_("bool"))),
    ]) {
        assert_path(test_struct(), path, expect);
    }
}

#[test]
fn vector_() {
    let expect = r#"
[0] vector<struct 0x0::foo::Quy {
    q: u8,
    r: bool,
}>
[1] struct 0x0::foo::Quy {
    q: u8,
    r: bool,
}
[2] 12: u8
[2] true: bool
[1] struct 0x0::foo::Quy {
    q: u8,
    r: bool,
}
[2] 13: u8
[2] false: bool
    "#;

    for path in enumerate_paths(vec![C::Opt(E::Type(&type_("vector<0x0::foo::Quy>")))]) {
        assert_path(test_vector(), path, expect);
    }
}

#[test]
fn vector_0() {
    let expect = r#"
[0] struct 0x0::foo::Quy {
    q: u8,
    r: bool,
}
[1] 12: u8
[1] true: bool
    "#;

    for path in enumerate_paths(vec![
        C::Opt(E::Type(&type_("vector<0x0::foo::Quy>"))),
        C::Req(vec![E::Index(0)]),
        C::Opt(E::Type(&type_("0x0::foo::Quy"))),
    ]) {
        assert_path(test_vector(), path, expect);
    }
}

#[test]
fn vector_1() {
    let expect = r#"
[0] struct 0x0::foo::Quy {
    q: u8,
    r: bool,
}
[1] 13: u8
[1] false: bool
    "#;

    for path in enumerate_paths(vec![
        C::Opt(E::Type(&type_("vector<0x0::foo::Quy>"))),
        C::Req(vec![E::Index(1)]),
        C::Opt(E::Type(&type_("0x0::foo::Quy"))),
    ]) {
        assert_path(test_vector(), path, expect);
    }
}

#[test]
fn enum_() {
    let expect = r#"
[0] enum 0x0::foo::Qux {
    n {
        o: u8,
    },
}
[1] 11: u8
    "#;

    for path in enumerate_paths(vec![
        C::Opt(E::Type(&type_("0x0::foo::Qux"))),
        C::Opt(E::Variant("n")),
    ]) {
        assert_path(test_enum(), path, expect);
    }
}

#[test]
fn enum_o() {
    let expect = r#"
[0] 11: u8
    "#;

    for path in enumerate_paths(vec![
        C::Opt(E::Type(&type_("0x0::foo::Qux"))),
        C::Opt(E::Variant("n")),
        C::Req(vec![E::Field("o"), E::Index(0)]),
        C::Opt(E::Type(&type_("u8"))),
    ]) {
        assert_path(test_enum(), path, expect);
    }
}

#[test]
fn field_not_found() {
    for path in [
        vec![E::Field("z")],
        // Trying to access a field on a primitive
        vec![E::Field("a"), E::Field("z")],
        // Nested field doesn't exist
        vec![E::Field("k"), E::Field("z")],
        // Nested field on an enum (that doesn't exist)
        vec![E::Field("m"), E::Field("z")],
        // Trying to access a field on a vector
        vec![E::Field("p"), E::Field("z")],
        // Nested field on a struct in a vector
        vec![E::Field("p"), E::Index(0), E::Field("z")],
    ] {
        assert_no_path(test_struct(), path);
    }
}

#[test]
fn index_out_of_bounds() {
    for path in [
        // Positional access of field, out of bounds
        vec![E::Index(1000)],
        // Trying to access index on a primitive
        vec![E::Field("a"), E::Index(1000)],
        // Out of bounds on primitive vector
        vec![E::Field("j"), E::Index(1000)],
        // Out of bounds field on nested struct
        vec![E::Field("k"), E::Index(1000)],
        // Out of bounds field on nested enum
        vec![E::Field("m"), E::Index(1000)],
        // Out of bounds field on struct vector
        vec![E::Field("p"), E::Index(1000)],
        // Out of bounds field on struct in vector
        vec![E::Field("p"), E::Index(0), E::Index(1000)],
    ] {
        assert_no_path(test_struct(), path);
    }
}

#[test]
fn type_mismatch() {
    for path in [
        // Wrong root type
        vec![E::Type(&type_("0x0::foo::Baz"))],
        // Wrong primitive type
        vec![E::Field("a"), E::Type(&type_("u16"))],
        // Wrong nested struct
        vec![E::Field("k"), E::Type(&type_("0x0::foo::Bar"))],
        // Wrong type with further nesting
        vec![
            E::Field("k"),
            E::Type(&type_("0x0::foo::Bar")),
            E::Field("l"),
        ],
        // Wrong primitive vector
        vec![E::Field("j"), E::Type(&type_("vector<u16>"))],
        vec![E::Field("j"), E::Type(&type_("u8"))],
        // Wrong enum type
        vec![E::Field("m"), E::Type(&type_("0x0::foo::Bar"))],
        // Wrong type nested inside enum
        vec![E::Field("m"), E::Field("o"), E::Type(&type_("u16"))],
    ] {
        assert_no_path(test_struct(), path);
    }
}

#[test]
fn variant_not_found() {
    assert_no_path(test_enum(), vec![E::Variant("z")]);
    assert_no_path(test_struct(), vec![E::Field("m"), E::Variant("z")]);
}

/// Components are used to generate paths. Each component offers a number of options for the
/// element that goes in the same position in the generated path.
enum C<'p> {
    /// This element is optional -- paths are geneated with and without this element at the
    /// component's position.
    Opt(E<'p>),

    /// This element is required, and is picked from the provided list.
    Req(Vec<E<'p>>),
}

/// Generate a list of paths as a cartesian product of the provided components.
fn enumerate_paths(components: Vec<C<'_>>) -> Vec<Vec<E<'_>>> {
    let mut paths = vec![vec![]];

    for component in components {
        let mut new_paths = vec![];

        for path in paths {
            match &component {
                C::Opt(element) => {
                    new_paths.push(path.clone());
                    let mut path = path.clone();
                    path.push(element.clone());
                    new_paths.push(path);
                }
                C::Req(elements) => {
                    new_paths.extend(elements.iter().map(|e| {
                        let mut path = path.clone();
                        path.push(e.clone());
                        path
                    }));
                }
            }
        }

        paths = new_paths;
    }

    paths
}

fn assert_path((value, layout): (MoveValue, MoveTypeLayout), path: Vec<E<'_>>, expect: &str) {
    let bytes = serialize(value);
    let mut printer = PrintVisitor::default();

    assert!(
        Extractor::deserialize_value(&bytes, &layout, &mut printer, path.clone())
            .unwrap()
            .is_some(),
        "Failed to extract value {path:?}",
    );

    assert_eq!(
        printer.output.trim(),
        expect.trim(),
        "Failed to match value at {path:?}"
    );
}

fn assert_no_path((value, layout): (MoveValue, MoveTypeLayout), path: Vec<E<'_>>) {
    let bytes = serialize(value);
    let mut printer = PrintVisitor::default();

    assert!(
        Extractor::deserialize_value(&bytes, &layout, &mut printer, path.clone())
            .unwrap()
            .is_none(),
        "Expected not to find something at {path:?}",
    );

    assert!(
        printer.output.is_empty(),
        "Expected not to delegate to the inner visitor for {path:?}"
    );
}

fn type_(t: &str) -> TypeTag {
    TypeTag::from_str(t).unwrap()
}

fn test_struct() -> (MoveValue, MoveTypeLayout) {
    use MoveTypeLayout as T;
    use MoveValue as V;

    let (vector, vector_layout) = test_vector();
    let (variant, enum_layout) = test_enum();

    let value = struct_value_(
        "0x0::foo::Bar",
        vec![
            ("a", V::U8(1)),
            ("b", V::U16(2)),
            ("c", V::U32(3)),
            ("d", V::U64(4)),
            ("e", V::U128(5)),
            ("f", V::U256(6u32.into())),
            ("g", V::Bool(true)),
            ("h", V::Address(AccountAddress::ZERO)),
            ("i", V::Signer(AccountAddress::ZERO)),
            ("j", V::Vector(vec![V::U8(7), V::U8(8), V::U8(9)])),
            ("k", struct_value_("0x0::foo::Baz", vec![("l", V::U8(10))])),
            ("m", variant),
            ("p", vector),
        ],
    );

    let layout = struct_layout_(
        "0x0::foo::Bar",
        vec![
            ("a", T::U8),
            ("b", T::U16),
            ("c", T::U32),
            ("d", T::U64),
            ("e", T::U128),
            ("f", T::U256),
            ("g", T::Bool),
            ("h", T::Address),
            ("i", T::Signer),
            ("j", T::Vector(Box::new(T::U8))),
            ("k", struct_layout_("0x0::foo::Baz", vec![("l", T::U8)])),
            ("m", enum_layout),
            ("p", vector_layout),
        ],
    );

    (value, layout)
}

fn test_enum() -> (MoveValue, MoveTypeLayout) {
    use MoveTypeLayout as T;
    use MoveValue as V;

    let value = variant_value_("0x0::foo::Qux", "n", 0, vec![("o", V::U8(11))]);
    let layout = enum_layout_("0x0::foo::Qux", vec![("n", vec![("o", T::U8)])]);

    (value, layout)
}

fn test_vector() -> (MoveValue, MoveTypeLayout) {
    use MoveTypeLayout as T;
    use MoveValue as V;

    let value = V::Vector(vec![
        struct_value_(
            "0x0::foo::Quy",
            vec![("q", V::U8(12)), ("r", V::Bool(true))],
        ),
        struct_value_(
            "0x0::foo::Quy",
            vec![("q", V::U8(13)), ("r", V::Bool(false))],
        ),
    ]);

    let layout = T::Vector(Box::new(struct_layout_(
        "0x0::foo::Quy",
        vec![("q", T::U8), ("r", T::Bool)],
    )));

    (value, layout)
}
