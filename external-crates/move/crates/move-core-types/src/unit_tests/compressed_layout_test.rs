// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

use crate::{
    account_address::AccountAddress,
    annotated_value::{self as A, compressed_layouts as AC},
    ident_str,
    identifier::Identifier,
    language_storage::StructTag,
    runtime_value::{self as R, compressed_layouts as RC},
};

fn test_struct_tag(name: &str) -> StructTag {
    StructTag {
        address: AccountAddress::ZERO,
        name: Identifier::new(name).unwrap(),
        module: ident_str!("test").to_owned(),
        type_params: vec![],
    }
}

// =============================================================================
// Runtime compressed layout tests
// =============================================================================

#[test]
fn runtime_primitive_roundtrip() {
    for layout in [
        R::MoveTypeLayout::Bool,
        R::MoveTypeLayout::U8,
        R::MoveTypeLayout::U16,
        R::MoveTypeLayout::U32,
        R::MoveTypeLayout::U64,
        R::MoveTypeLayout::U128,
        R::MoveTypeLayout::U256,
        R::MoveTypeLayout::Address,
        R::MoveTypeLayout::Signer,
    ] {
        let compressed = RC::MoveTypeLayout::try_from(&layout).unwrap();
        assert_eq!(compressed.node_count(), 0);
        let inflated = compressed.inflate().unwrap();
        assert_eq!(format!("{inflated}"), format!("{layout}"));
    }
}

#[test]
fn runtime_vector_roundtrip() {
    let layout = R::MoveTypeLayout::Vector(Box::new(R::MoveTypeLayout::U8));
    let compressed = RC::MoveTypeLayout::try_from(&layout).unwrap();
    assert_eq!(compressed.node_count(), 1);
    let inflated = compressed.inflate().unwrap();
    assert_eq!(format!("{inflated}"), format!("{layout}"));
}

#[test]
fn runtime_nested_vector_roundtrip() {
    let layout = R::MoveTypeLayout::Vector(Box::new(R::MoveTypeLayout::Vector(Box::new(
        R::MoveTypeLayout::U64,
    ))));
    let compressed = RC::MoveTypeLayout::try_from(&layout).unwrap();
    assert_eq!(compressed.node_count(), 2);
    let inflated = compressed.inflate().unwrap();
    assert_eq!(format!("{inflated}"), format!("{layout}"));
}

#[test]
fn runtime_struct_roundtrip() {
    let layout = R::MoveTypeLayout::Struct(Box::new(R::MoveStructLayout::new(vec![
        R::MoveTypeLayout::U64,
        R::MoveTypeLayout::Bool,
        R::MoveTypeLayout::Address,
    ])));
    let compressed = RC::MoveTypeLayout::try_from(&layout).unwrap();
    assert_eq!(compressed.node_count(), 1);
    let inflated = compressed.inflate().unwrap();
    assert_eq!(format!("{inflated}"), format!("{layout}"));
}

#[test]
fn runtime_enum_roundtrip() {
    let layout = R::MoveTypeLayout::Enum(Box::new(R::MoveEnumLayout(Box::new(vec![
        vec![R::MoveTypeLayout::U8],
        vec![R::MoveTypeLayout::Bool, R::MoveTypeLayout::U64],
    ]))));
    let compressed = RC::MoveTypeLayout::try_from(&layout).unwrap();
    assert_eq!(compressed.node_count(), 1);
    let inflated = compressed.inflate().unwrap();
    assert_eq!(format!("{inflated}"), format!("{layout}"));
}

#[test]
fn runtime_primitive_dedup() {
    let layout = R::MoveTypeLayout::Struct(Box::new(R::MoveStructLayout::new(vec![
        R::MoveTypeLayout::U64,
        R::MoveTypeLayout::U64,
    ])));
    let compressed = RC::MoveTypeLayout::try_from(&layout).unwrap();
    assert_eq!(compressed.node_count(), 1);
}

#[test]
fn runtime_struct_dedup() {
    let inner = R::MoveStructLayout::new(vec![R::MoveTypeLayout::U64]);
    let layout = R::MoveTypeLayout::Struct(Box::new(R::MoveStructLayout::new(vec![
        R::MoveTypeLayout::Struct(Box::new(inner.clone())),
        R::MoveTypeLayout::Struct(Box::new(inner)),
    ])));
    let compressed = RC::MoveTypeLayout::try_from(&layout).unwrap();
    assert_eq!(compressed.node_count(), 2);
}

#[test]
fn runtime_shared_subtree_dedup() {
    let inner = R::MoveStructLayout::new(vec![R::MoveTypeLayout::U64, R::MoveTypeLayout::Bool]);
    let wrapper = R::MoveTypeLayout::Struct(Box::new(R::MoveStructLayout::new(vec![
        R::MoveTypeLayout::Struct(Box::new(inner.clone())),
        R::MoveTypeLayout::Struct(Box::new(inner)),
    ])));
    let compressed = RC::MoveTypeLayout::try_from(&wrapper).unwrap();
    assert_eq!(compressed.node_count(), 2);
    let inflated = compressed.inflate().unwrap();
    assert_eq!(format!("{inflated}"), format!("{wrapper}"));
}

#[test]
fn runtime_empty_struct() {
    let layout = R::MoveTypeLayout::Struct(Box::new(R::MoveStructLayout::new(vec![])));
    let compressed = RC::MoveTypeLayout::try_from(&layout).unwrap();
    assert_eq!(compressed.node_count(), 1);
    let inflated = compressed.inflate().unwrap();
    assert_eq!(format!("{inflated}"), format!("{layout}"));
}

#[test]
fn runtime_deeply_nested_vector() {
    let mut layout = R::MoveTypeLayout::U8;
    for _ in 0..3 {
        layout = R::MoveTypeLayout::Vector(Box::new(layout));
    }
    let compressed = RC::MoveTypeLayout::try_from(&layout).unwrap();
    assert_eq!(compressed.node_count(), 3);
    let inflated = compressed.inflate().unwrap();
    assert_eq!(format!("{inflated}"), format!("{layout}"));
}

// =============================================================================
// Annotated compressed layout tests
// =============================================================================

#[test]
fn annotated_primitive_roundtrip() {
    for layout in [
        A::MoveTypeLayout::Bool,
        A::MoveTypeLayout::U8,
        A::MoveTypeLayout::U16,
        A::MoveTypeLayout::U32,
        A::MoveTypeLayout::U64,
        A::MoveTypeLayout::U128,
        A::MoveTypeLayout::U256,
        A::MoveTypeLayout::Address,
        A::MoveTypeLayout::Signer,
    ] {
        let compressed = AC::MoveTypeLayout::try_from(&layout).unwrap();
        assert_eq!(compressed.node_count(), 0);
        let inflated = compressed.inflate().unwrap();
        assert_eq!(inflated, layout);
    }
}

#[test]
fn annotated_struct_roundtrip() {
    let layout = A::MoveTypeLayout::Struct(Box::new(A::MoveStructLayout {
        type_: test_struct_tag("Foo"),
        fields: vec![
            A::MoveFieldLayout::new(ident_str!("x").to_owned(), A::MoveTypeLayout::U64),
            A::MoveFieldLayout::new(ident_str!("y").to_owned(), A::MoveTypeLayout::Bool),
        ],
    }));
    let compressed = AC::MoveTypeLayout::try_from(&layout).unwrap();
    assert_eq!(compressed.node_count(), 1);
    let inflated = compressed.inflate().unwrap();
    assert_eq!(inflated, layout);
}

#[test]
fn annotated_nested_struct_roundtrip() {
    let layout = A::MoveTypeLayout::Struct(Box::new(A::MoveStructLayout {
        type_: test_struct_tag("Outer"),
        fields: vec![
            A::MoveFieldLayout::new(
                ident_str!("a").to_owned(),
                A::MoveTypeLayout::Struct(Box::new(A::MoveStructLayout {
                    type_: test_struct_tag("Inner1"),
                    fields: vec![A::MoveFieldLayout::new(
                        ident_str!("id").to_owned(),
                        A::MoveTypeLayout::Address,
                    )],
                })),
            ),
            A::MoveFieldLayout::new(
                ident_str!("b").to_owned(),
                A::MoveTypeLayout::Struct(Box::new(A::MoveStructLayout {
                    type_: test_struct_tag("Inner2"),
                    fields: vec![A::MoveFieldLayout::new(
                        ident_str!("id").to_owned(),
                        A::MoveTypeLayout::Address,
                    )],
                })),
            ),
        ],
    }));
    let compressed = AC::MoveTypeLayout::try_from(&layout).unwrap();

    let inflated = compressed.inflate().unwrap();
    assert_eq!(inflated, layout);
}

#[test]
fn annotated_enum_roundtrip() {
    let layout = A::MoveTypeLayout::Enum(Box::new(A::MoveEnumLayout {
        type_: test_struct_tag("MyEnum"),
        variants: [
            ((ident_str!("None").to_owned(), 0), vec![]),
            (
                (ident_str!("Some").to_owned(), 1),
                vec![A::MoveFieldLayout::new(
                    ident_str!("value").to_owned(),
                    A::MoveTypeLayout::U64,
                )],
            ),
        ]
        .into_iter()
        .collect(),
    }));
    let compressed = AC::MoveTypeLayout::try_from(&layout).unwrap();
    let inflated = compressed.inflate().unwrap();
    assert_eq!(inflated, layout);
}

#[test]
fn annotated_shared_subtree_dedup() {
    let inner = A::MoveStructLayout {
        type_: test_struct_tag("Inner"),
        fields: vec![
            A::MoveFieldLayout::new(ident_str!("a").to_owned(), A::MoveTypeLayout::U64),
            A::MoveFieldLayout::new(ident_str!("b").to_owned(), A::MoveTypeLayout::Bool),
        ],
    };
    let layout = A::MoveTypeLayout::Struct(Box::new(A::MoveStructLayout {
        type_: test_struct_tag("Wrapper"),
        fields: vec![
            A::MoveFieldLayout::new(
                ident_str!("x").to_owned(),
                A::MoveTypeLayout::Struct(Box::new(inner.clone())),
            ),
            A::MoveFieldLayout::new(
                ident_str!("y").to_owned(),
                A::MoveTypeLayout::Struct(Box::new(inner)),
            ),
        ],
    }));
    let compressed = AC::MoveTypeLayout::try_from(&layout).unwrap();

    // 2 nodes: Inner struct, Wrapper struct (Inner deduped; primitives are inline)
    assert_eq!(compressed.node_count(), 2);
    let inflated = compressed.inflate().unwrap();
    assert_eq!(inflated, layout);
}

// =============================================================================
// Layout navigation tests
// =============================================================================

#[test]
fn runtime_view_inflate_matches_layout_inflate() {
    let inner = R::MoveStructLayout::new(vec![R::MoveTypeLayout::U64, R::MoveTypeLayout::Bool]);
    let layout = R::MoveTypeLayout::Struct(Box::new(R::MoveStructLayout::new(vec![
        R::MoveTypeLayout::Struct(Box::new(inner.clone())),
        R::MoveTypeLayout::Struct(Box::new(inner)),
    ])));
    let compressed = RC::MoveTypeLayout::try_from(&layout).unwrap();
    let view = compressed.as_view();
    assert_eq!(
        format!("{}", view.inflate().unwrap()),
        format!("{}", compressed.inflate().unwrap())
    );
}

#[test]
fn runtime_struct_view_navigate() {
    let layout = R::MoveTypeLayout::Struct(Box::new(R::MoveStructLayout::new(vec![
        R::MoveTypeLayout::U64,
        R::MoveTypeLayout::Bool,
    ])));
    let compressed = RC::MoveTypeLayout::try_from(&layout).unwrap();
    let view = compressed.as_view();
    let fv = match &view {
        RC::MoveLayoutView::Struct(fv) => fv,
        _ => panic!("expected struct"),
    };
    assert_eq!(fv.field_count(), 2);

    let field0 = fv.field(0).unwrap();
    assert_eq!(format!("{}", field0.inflate().unwrap()), "u64");
    let field1 = fv.field(1).unwrap();
    assert_eq!(format!("{}", field1.inflate().unwrap()), "bool");
}

#[test]
fn runtime_enum_view_navigate() {
    let layout = R::MoveTypeLayout::Enum(Box::new(R::MoveEnumLayout(Box::new(vec![
        vec![R::MoveTypeLayout::U8],
        vec![R::MoveTypeLayout::Bool, R::MoveTypeLayout::U64],
    ]))));
    let compressed = RC::MoveTypeLayout::try_from(&layout).unwrap();
    let view = compressed.as_view();
    let ev = match &view {
        RC::MoveLayoutView::Enum(ev) => ev,
        _ => panic!("expected enum"),
    };
    assert_eq!(ev.variant_count(), 2);

    let v0 = match ev.variant(0).unwrap() {
        RC::VariantLayout::Known(fv) => fv,
        RC::VariantLayout::Unknown => panic!("expected known variant"),
    };
    assert_eq!(v0.field_count(), 1);
    assert_eq!(format!("{}", v0.field(0).unwrap().inflate().unwrap()), "u8");

    let v1 = match ev.variant(1).unwrap() {
        RC::VariantLayout::Known(fv) => fv,
        RC::VariantLayout::Unknown => panic!("expected known variant"),
    };
    assert_eq!(v1.field_count(), 2);
}

#[test]
fn runtime_vector_element_navigate() {
    let inner = R::MoveStructLayout::new(vec![R::MoveTypeLayout::U64, R::MoveTypeLayout::Bool]);
    let layout = R::MoveTypeLayout::Vector(Box::new(R::MoveTypeLayout::Struct(Box::new(inner))));
    let compressed = RC::MoveTypeLayout::try_from(&layout).unwrap();
    let view = compressed.as_view();

    let vv = match &view {
        RC::MoveLayoutView::Vector(vv) => vv,
        _ => panic!("expected vector"),
    };
    let fv = match vv.as_view() {
        RC::MoveLayoutView::Struct(fv) => fv,
        _ => panic!("expected struct"),
    };
    assert_eq!(fv.field_count(), 2);
}

#[test]
fn annotated_view_struct_navigate() {
    let layout = A::MoveTypeLayout::Struct(Box::new(A::MoveStructLayout {
        type_: test_struct_tag("Foo"),
        fields: vec![
            A::MoveFieldLayout::new(ident_str!("x").to_owned(), A::MoveTypeLayout::U64),
            A::MoveFieldLayout::new(ident_str!("y").to_owned(), A::MoveTypeLayout::Bool),
        ],
    }));
    let compressed = AC::MoveTypeLayout::try_from(&layout).unwrap();
    let view = compressed.as_view();
    assert_eq!(view.inflate().unwrap(), layout);

    let sv = match &view {
        AC::MoveLayoutView::Struct(sv) => sv,
        _ => panic!("expected struct"),
    };
    assert_eq!(sv.type_().name.as_str(), "Foo");
    assert_eq!(sv.field_count(), 2);

    let (name, field_view) = sv.field(0).unwrap();
    assert_eq!(name.as_str(), "x");
    assert_eq!(field_view.inflate().unwrap(), A::MoveTypeLayout::U64);
}

#[test]
fn annotated_view_enum_navigate() {
    let layout = A::MoveTypeLayout::Enum(Box::new(A::MoveEnumLayout {
        type_: test_struct_tag("MyEnum"),
        variants: [
            ((ident_str!("None").to_owned(), 0), vec![]),
            (
                (ident_str!("Some").to_owned(), 1),
                vec![A::MoveFieldLayout::new(
                    ident_str!("value").to_owned(),
                    A::MoveTypeLayout::U64,
                )],
            ),
        ]
        .into_iter()
        .collect(),
    }));
    let compressed = AC::MoveTypeLayout::try_from(&layout).unwrap();
    let view = compressed.as_view();
    let ev = match &view {
        AC::MoveLayoutView::Enum(ev) => ev,
        _ => panic!("expected enum"),
    };
    assert_eq!(ev.type_().name.as_str(), "MyEnum");

    let vl = ev.variant_by_tag(1).unwrap();
    assert_eq!(vl.name().as_str(), "Some");
    let fv = vl.fields().expect("expected known variant");
    assert_eq!(fv.field_count(), 1);
    let (field_name, field_layout) = fv.field(0).unwrap();
    assert_eq!(field_name.as_str(), "value");
    assert_eq!(field_layout.inflate().unwrap(), A::MoveTypeLayout::U64);

    let vl = ev.variant_by_tag(0).unwrap();
    assert_eq!(vl.name().as_str(), "None");
    let fv = vl.fields().expect("expected known variant");
    assert_eq!(fv.field_count(), 0);
}

// =============================================================================
// Unknown variant tests
// =============================================================================

#[test]
fn runtime_unknown_variant_inflate_fails() {
    let mut b = RC::MoveTypeLayoutBuilder::new();
    let u64_h = b.u64();
    // variant 0: known (one u64 field), variant 1: unknown
    let handle = b.enum_layout(&[Some(&[u64_h]), None]).unwrap();
    let layout = b.build(handle);

    let err = layout.inflate().unwrap_err();
    assert!(
        err.to_string().contains("unknown variant"),
        "expected unknown variant error, got: {err}"
    );
}

#[test]
fn runtime_unknown_variant_deser_fails() {
    let mut b = RC::MoveTypeLayoutBuilder::new();
    let u64_h = b.u64();
    // variant 0: known, variant 1: unknown
    let handle = b.enum_layout(&[Some(&[u64_h]), None]).unwrap();
    let layout = b.build(handle);

    // BCS for variant tag=1, followed by a u64
    let blob = bcs::to_bytes(&(1u8, (42u64,))).unwrap();
    let err = bcs::from_bytes_seed(&layout, &blob)
        .map(|_: R::MoveValue| ())
        .unwrap_err();
    assert!(
        err.to_string().contains("layout unknown"),
        "expected layout unknown error, got: {err}"
    );
}

#[test]
fn runtime_unknown_variant_known_variant_deser_succeeds() {
    let mut b = RC::MoveTypeLayoutBuilder::new();
    let u64_h = b.u64();
    // variant 0: known (one u64 field), variant 1: unknown
    let handle = b.enum_layout(&[Some(&[u64_h]), None]).unwrap();
    let layout = b.build(handle);

    // BCS for variant tag=0 with one u64 field — should succeed
    let blob = bcs::to_bytes(&(0u8, (999u64,))).unwrap();
    let result: R::MoveValue = bcs::from_bytes_seed(&layout, &blob).unwrap();
    assert_eq!(
        result,
        R::MoveValue::Variant(R::MoveVariant {
            tag: 0,
            fields: vec![R::MoveValue::U64(999)],
        })
    );
}

#[test]
fn annotated_unknown_variant_inflate_fails() {
    let mut b = AC::MoveTypeLayoutBuilder::new();
    let u64_h = b.u64();
    let some_name = Identifier::new("Some").unwrap();
    let none_name = Identifier::new("None").unwrap();
    let value_name = Identifier::new("value").unwrap();
    // variant 0 "Some": known, variant 1 "None": unknown
    let handle = b
        .enum_layout(
            &test_struct_tag("MyEnum"),
            &[
                (&some_name, 0, Some(&[(&value_name, u64_h)])),
                (&none_name, 1, None),
            ],
        )
        .unwrap();
    let layout = b.build(handle);

    let err = layout.inflate().unwrap_err();
    assert!(
        err.to_string().contains("unknown variant"),
        "expected unknown variant error, got: {err}"
    );
}

#[test]
fn annotated_unknown_variant_deser_fails() {
    let mut b = AC::MoveTypeLayoutBuilder::new();
    let u64_h = b.u64();
    let some_name = Identifier::new("Some").unwrap();
    let none_name = Identifier::new("None").unwrap();
    let value_name = Identifier::new("value").unwrap();
    // variant 0 "Some": known, variant 1 "None": unknown
    let handle = b
        .enum_layout(
            &test_struct_tag("MyEnum"),
            &[
                (&some_name, 0, Some(&[(&value_name, u64_h)])),
                (&none_name, 1, None),
            ],
        )
        .unwrap();
    let layout = b.build(handle);

    // BCS for variant tag=1 (the unknown variant)
    let blob = bcs::to_bytes(&(1u8, ())).unwrap();
    let err = bcs::from_bytes_seed(&layout, &blob)
        .map(|_: A::MoveValue| ())
        .unwrap_err();
    assert!(
        err.to_string().contains("layout unknown"),
        "expected layout unknown error, got: {err}"
    );
}

#[test]
fn annotated_unknown_variant_known_variant_deser_succeeds() {
    let mut b = AC::MoveTypeLayoutBuilder::new();
    let u64_h = b.u64();
    let some_name = Identifier::new("Some").unwrap();
    let none_name = Identifier::new("None").unwrap();
    let value_name = Identifier::new("value").unwrap();
    // variant 0 "Some": known, variant 1 "None": unknown
    let handle = b
        .enum_layout(
            &test_struct_tag("MyEnum"),
            &[
                (&some_name, 0, Some(&[(&value_name, u64_h)])),
                (&none_name, 1, None),
            ],
        )
        .unwrap();
    let layout = b.build(handle);

    // BCS for variant tag=0 with one u64 field — should succeed
    let blob = bcs::to_bytes(&(0u8, (42u64,))).unwrap();
    let result: A::MoveValue = bcs::from_bytes_seed(&layout, &blob).unwrap();
    match result {
        A::MoveValue::Variant(v) => {
            assert_eq!(v.variant_name.as_str(), "Some");
            assert_eq!(v.tag, 0);
            assert_eq!(v.fields.len(), 1);
            assert_eq!(v.fields[0].1, A::MoveValue::U64(42));
        }
        _ => panic!("expected variant"),
    }
}

// =============================================================================
// Deserialization parity tests — tree vs compressed produce identical results
// =============================================================================

/// Helper: given a runtime layout and value, assert that tree-based and
/// compressed deserialization produce the exact same MoveValue.
fn assert_runtime_deser_parity(layout: &R::MoveTypeLayout, value: &R::MoveValue) {
    let blob = value.simple_serialize().unwrap();
    let tree_result = R::MoveValue::simple_deserialize(&blob, layout).unwrap();
    let compressed = RC::MoveTypeLayout::try_from(layout).unwrap();
    let compressed_result: R::MoveValue = bcs::from_bytes_seed(&compressed, &blob).unwrap();
    assert_eq!(
        tree_result, compressed_result,
        "tree vs compressed mismatch for layout {:?}",
        layout
    );
}

/// Helper: given an annotated layout and its BCS blob, assert that tree-based
/// and compressed deserialization produce the exact same annotated MoveValue.
fn assert_annotated_deser_parity(layout: &A::MoveTypeLayout, blob: &[u8]) {
    let tree_result: A::MoveValue = bcs::from_bytes_seed(layout, blob).unwrap();
    let compressed = AC::MoveTypeLayout::try_from(layout).unwrap();
    let compressed_result: A::MoveValue = bcs::from_bytes_seed(&compressed, blob).unwrap();
    assert_eq!(
        tree_result, compressed_result,
        "tree vs compressed mismatch for annotated layout"
    );
}

#[test]
fn runtime_deser_parity_primitives() {
    let cases: Vec<(R::MoveTypeLayout, R::MoveValue)> = vec![
        (R::MoveTypeLayout::Bool, R::MoveValue::Bool(true)),
        (R::MoveTypeLayout::Bool, R::MoveValue::Bool(false)),
        (R::MoveTypeLayout::U8, R::MoveValue::U8(0)),
        (R::MoveTypeLayout::U8, R::MoveValue::U8(255)),
        (R::MoveTypeLayout::U16, R::MoveValue::U16(12345)),
        (R::MoveTypeLayout::U32, R::MoveValue::U32(0xDEADBEEF)),
        (R::MoveTypeLayout::U64, R::MoveValue::U64(u64::MAX)),
        (R::MoveTypeLayout::U128, R::MoveValue::U128(u128::MAX)),
        (
            R::MoveTypeLayout::Address,
            R::MoveValue::Address(AccountAddress::ZERO),
        ),
        (
            R::MoveTypeLayout::Address,
            R::MoveValue::Address(AccountAddress::ONE),
        ),
        (
            R::MoveTypeLayout::Signer,
            R::MoveValue::Signer(AccountAddress::ZERO),
        ),
    ];
    for (layout, value) in &cases {
        assert_runtime_deser_parity(layout, value);
    }
}

#[test]
fn runtime_deser_parity_vectors() {
    // vector<u64>
    let layout = R::MoveTypeLayout::Vector(Box::new(R::MoveTypeLayout::U64));
    let value = R::MoveValue::Vector(vec![
        R::MoveValue::U64(1),
        R::MoveValue::U64(2),
        R::MoveValue::U64(3),
    ]);
    assert_runtime_deser_parity(&layout, &value);

    // empty vector
    let empty = R::MoveValue::Vector(vec![]);
    assert_runtime_deser_parity(&layout, &empty);

    // vector<vector<bool>>
    let nested_layout = R::MoveTypeLayout::Vector(Box::new(R::MoveTypeLayout::Vector(Box::new(
        R::MoveTypeLayout::Bool,
    ))));
    let nested_value = R::MoveValue::Vector(vec![
        R::MoveValue::Vector(vec![R::MoveValue::Bool(true), R::MoveValue::Bool(false)]),
        R::MoveValue::Vector(vec![]),
    ]);
    assert_runtime_deser_parity(&nested_layout, &nested_value);

    // vector<u8> (byte vector)
    let byte_layout = R::MoveTypeLayout::Vector(Box::new(R::MoveTypeLayout::U8));
    let byte_value = R::MoveValue::Vector(vec![
        R::MoveValue::U8(0xDE),
        R::MoveValue::U8(0xAD),
        R::MoveValue::U8(0xBE),
        R::MoveValue::U8(0xEF),
    ]);
    assert_runtime_deser_parity(&byte_layout, &byte_value);
}

#[test]
fn runtime_deser_parity_structs() {
    // simple struct
    let layout = R::MoveTypeLayout::Struct(Box::new(R::MoveStructLayout::new(vec![
        R::MoveTypeLayout::U64,
        R::MoveTypeLayout::Bool,
    ])));
    let value = R::MoveValue::Struct(R::MoveStruct(vec![
        R::MoveValue::U64(100),
        R::MoveValue::Bool(false),
    ]));
    assert_runtime_deser_parity(&layout, &value);

    // empty struct
    let empty_layout = R::MoveTypeLayout::Struct(Box::new(R::MoveStructLayout::new(vec![])));
    let empty_value = R::MoveValue::Struct(R::MoveStruct(vec![]));
    assert_runtime_deser_parity(&empty_layout, &empty_value);

    // nested struct
    let inner_layout = R::MoveStructLayout::new(vec![R::MoveTypeLayout::U64]);
    let outer_layout = R::MoveTypeLayout::Struct(Box::new(R::MoveStructLayout::new(vec![
        R::MoveTypeLayout::Struct(Box::new(inner_layout)),
        R::MoveTypeLayout::Bool,
    ])));
    let outer_value = R::MoveValue::Struct(R::MoveStruct(vec![
        R::MoveValue::Struct(R::MoveStruct(vec![R::MoveValue::U64(42)])),
        R::MoveValue::Bool(true),
    ]));
    assert_runtime_deser_parity(&outer_layout, &outer_value);

    // struct with vector field
    let vec_field_layout = R::MoveTypeLayout::Struct(Box::new(R::MoveStructLayout::new(vec![
        R::MoveTypeLayout::Vector(Box::new(R::MoveTypeLayout::U8)),
        R::MoveTypeLayout::Address,
    ])));
    let vec_field_value = R::MoveValue::Struct(R::MoveStruct(vec![
        R::MoveValue::Vector(vec![R::MoveValue::U8(1), R::MoveValue::U8(2)]),
        R::MoveValue::Address(AccountAddress::ONE),
    ]));
    assert_runtime_deser_parity(&vec_field_layout, &vec_field_value);
}

#[test]
fn runtime_deser_parity_enums() {
    let layout = R::MoveTypeLayout::Enum(Box::new(R::MoveEnumLayout(Box::new(vec![
        vec![],                                               // variant 0: no fields
        vec![R::MoveTypeLayout::U64],                         // variant 1: one field
        vec![R::MoveTypeLayout::Bool, R::MoveTypeLayout::U8], // variant 2: two fields
    ]))));

    // variant 0 (empty)
    let v0 = R::MoveValue::Variant(R::MoveVariant {
        tag: 0,
        fields: vec![],
    });
    assert_runtime_deser_parity(&layout, &v0);

    // variant 1
    let v1 = R::MoveValue::Variant(R::MoveVariant {
        tag: 1,
        fields: vec![R::MoveValue::U64(999)],
    });
    assert_runtime_deser_parity(&layout, &v1);

    // variant 2
    let v2 = R::MoveValue::Variant(R::MoveVariant {
        tag: 2,
        fields: vec![R::MoveValue::Bool(true), R::MoveValue::U8(42)],
    });
    assert_runtime_deser_parity(&layout, &v2);
}

#[test]
fn runtime_deser_parity_complex_nested() {
    // vector<struct { u64, enum { variant0(), variant1(bool) } }>
    let enum_layout = R::MoveEnumLayout(Box::new(vec![vec![], vec![R::MoveTypeLayout::Bool]]));
    let struct_layout = R::MoveStructLayout::new(vec![
        R::MoveTypeLayout::U64,
        R::MoveTypeLayout::Enum(Box::new(enum_layout)),
    ]);
    let layout =
        R::MoveTypeLayout::Vector(Box::new(R::MoveTypeLayout::Struct(Box::new(struct_layout))));

    let value = R::MoveValue::Vector(vec![
        R::MoveValue::Struct(R::MoveStruct(vec![
            R::MoveValue::U64(1),
            R::MoveValue::Variant(R::MoveVariant {
                tag: 0,
                fields: vec![],
            }),
        ])),
        R::MoveValue::Struct(R::MoveStruct(vec![
            R::MoveValue::U64(2),
            R::MoveValue::Variant(R::MoveVariant {
                tag: 1,
                fields: vec![R::MoveValue::Bool(true)],
            }),
        ])),
    ]);
    assert_runtime_deser_parity(&layout, &value);
}

#[test]
fn annotated_deser_parity_primitives() {
    let cases: Vec<(A::MoveTypeLayout, A::MoveValue)> = vec![
        (A::MoveTypeLayout::Bool, A::MoveValue::Bool(true)),
        (A::MoveTypeLayout::U8, A::MoveValue::U8(42)),
        (A::MoveTypeLayout::U64, A::MoveValue::U64(u64::MAX)),
        (A::MoveTypeLayout::U128, A::MoveValue::U128(u128::MAX)),
        (
            A::MoveTypeLayout::Address,
            A::MoveValue::Address(AccountAddress::ONE),
        ),
    ];
    for (layout, value) in &cases {
        let blob = bcs::to_bytes(value).unwrap();
        assert_annotated_deser_parity(layout, &blob);
    }
}

#[test]
fn annotated_deser_parity_struct() {
    let layout = A::MoveTypeLayout::Struct(Box::new(A::MoveStructLayout {
        type_: test_struct_tag("MyStruct"),
        fields: vec![
            A::MoveFieldLayout::new(ident_str!("f").to_owned(), A::MoveTypeLayout::U64),
            A::MoveFieldLayout::new(ident_str!("g").to_owned(), A::MoveTypeLayout::Bool),
        ],
    }));
    // Serialize the raw values (BCS doesn't include field names)
    let blob = bcs::to_bytes(&(7u64, true)).unwrap();
    assert_annotated_deser_parity(&layout, &blob);
}

#[test]
fn annotated_deser_parity_enum() {
    let layout = A::MoveTypeLayout::Enum(Box::new(A::MoveEnumLayout {
        type_: test_struct_tag("MyEnum"),
        variants: vec![
            (
                (ident_str!("Variant1").to_owned(), 0u16),
                vec![
                    A::MoveFieldLayout::new(ident_str!("f").to_owned(), A::MoveTypeLayout::U64),
                    A::MoveFieldLayout::new(ident_str!("g").to_owned(), A::MoveTypeLayout::Bool),
                ],
            ),
            (
                (ident_str!("Variant2").to_owned(), 1u16),
                vec![
                    A::MoveFieldLayout::new(ident_str!("f2").to_owned(), A::MoveTypeLayout::U64),
                    A::MoveFieldLayout::new(ident_str!("g2").to_owned(), A::MoveTypeLayout::Bool),
                    A::MoveFieldLayout::new(ident_str!("h2").to_owned(), A::MoveTypeLayout::U8),
                ],
            ),
        ]
        .into_iter()
        .collect(),
    }));

    // variant 0
    let blob0 = bcs::to_bytes(&(0u8, (7u64, true))).unwrap();
    assert_annotated_deser_parity(&layout, &blob0);

    // variant 1
    let blob1 = bcs::to_bytes(&(1u8, (8u64, false, 0u8))).unwrap();
    assert_annotated_deser_parity(&layout, &blob1);
}

#[test]
fn annotated_deser_parity_nested_struct() {
    let inner_layout = A::MoveStructLayout {
        type_: test_struct_tag("Inner"),
        fields: vec![A::MoveFieldLayout::new(
            ident_str!("val").to_owned(),
            A::MoveTypeLayout::U64,
        )],
    };
    let layout = A::MoveTypeLayout::Struct(Box::new(A::MoveStructLayout {
        type_: test_struct_tag("Outer"),
        fields: vec![
            A::MoveFieldLayout::new(
                ident_str!("inner").to_owned(),
                A::MoveTypeLayout::Struct(Box::new(inner_layout)),
            ),
            A::MoveFieldLayout::new(ident_str!("flag").to_owned(), A::MoveTypeLayout::Bool),
        ],
    }));
    // nested tuple: ((u64,), bool)
    let blob = bcs::to_bytes(&((42u64,), true)).unwrap();
    assert_annotated_deser_parity(&layout, &blob);
}

#[test]
fn annotated_deser_parity_vector() {
    let layout = A::MoveTypeLayout::Vector(Box::new(A::MoveTypeLayout::U64));
    let blob = bcs::to_bytes(&vec![1u64, 2u64, 3u64]).unwrap();
    assert_annotated_deser_parity(&layout, &blob);

    // empty vector
    let empty_blob = bcs::to_bytes(&Vec::<u64>::new()).unwrap();
    assert_annotated_deser_parity(&layout, &empty_blob);

    // vector of structs
    let struct_layout = A::MoveStructLayout {
        type_: test_struct_tag("Item"),
        fields: vec![A::MoveFieldLayout::new(
            ident_str!("x").to_owned(),
            A::MoveTypeLayout::U8,
        )],
    };
    let vec_struct_layout =
        A::MoveTypeLayout::Vector(Box::new(A::MoveTypeLayout::Struct(Box::new(struct_layout))));
    let blob = bcs::to_bytes(&vec![(1u8,), (2u8,)]).unwrap();
    assert_annotated_deser_parity(&vec_struct_layout, &blob);
}
