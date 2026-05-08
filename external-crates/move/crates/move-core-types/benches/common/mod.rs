// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

//! Shared bench scaffolding: shape catalog and annotated-layout builders
//! used by every bench file.
//!
//! ## Shapes
//!
//! Each shape exercises a different cost dimension of layout traversal:
//!
//! | Shape            | Description                                                      | Stresses                                          |
//! |------------------|------------------------------------------------------------------|---------------------------------------------------|
//! | `leaf`           | A single primitive (`u64`)                                       | Per-call overhead; baseline                       |
//! | `shallow_struct` | One struct with 8 mixed-primitive fields                         | Per-field iteration                               |
//! | `wide_struct`    | One struct with 64 mixed-primitive fields (8 cycles of 8 types)  | Iteration cost dominated by field count           |
//! | `deep_nested`    | 16 levels of `Struct { f: Struct { f: ... u64 } }`               | Recursive descent / per-level layout machinery    |
//! | `wide_enum`      | Enum with 32 known variants, each holding 4 primitive fields     | `as_view()` and `variant_by_tag()` on wide enums  |
//! | `realistic`      | Approximation of a Sui object: `UID` + `Balance` + `vector<u8>` + a small `Status` enum | Mixed shape similar to production data |

use move_core_types::{
    account_address::AccountAddress,
    compressed::annotated::{LayoutHandle, MoveTypeLayout, MoveTypeLayoutBuilder},
    identifier::Identifier,
    language_storage::StructTag,
};

/// Names of the shapes recognized by [`annotated_layout`]. Listed in the
/// order benches iterate them.
pub const SHAPE_NAMES: &[&str] = &[
    "leaf",
    "shallow_struct",
    "wide_struct",
    "deep_nested",
    "wide_enum",
    "realistic",
];

fn ident(s: &str) -> Identifier {
    Identifier::new(s).unwrap()
}

fn st(name: &str) -> StructTag {
    StructTag {
        address: AccountAddress::ONE,
        module: ident("m"),
        name: ident(name),
        type_params: vec![],
    }
}

/// Build the annotated [`MoveTypeLayout`] for a named shape from
/// [`SHAPE_NAMES`]. Panics on unknown name.
pub fn annotated_layout(name: &str) -> MoveTypeLayout {
    match name {
        "leaf" => leaf(),
        "shallow_struct" => shallow_struct(),
        "wide_struct" => wide_struct(),
        "deep_nested" => deep_nested(),
        "wide_enum" => wide_enum(),
        "realistic" => realistic(),
        _ => panic!("unknown shape: {name}"),
    }
}

fn leaf() -> MoveTypeLayout {
    MoveTypeLayout::u64()
}

fn shallow_struct() -> MoveTypeLayout {
    MoveTypeLayoutBuilder::with_builder::<_, anyhow::Error>(|b| {
        let fields = vec![
            (ident("f0"), b.bool()),
            (ident("f1"), b.u8()),
            (ident("f2"), b.u16()),
            (ident("f3"), b.u32()),
            (ident("f4"), b.u64()),
            (ident("f5"), b.u128()),
            (ident("f6"), b.address()),
            (ident("f7"), b.u256()),
        ];
        b.struct_layout(st("Shallow"), fields)
    })
    .unwrap()
}

fn wide_struct() -> MoveTypeLayout {
    MoveTypeLayoutBuilder::with_builder::<_, anyhow::Error>(|b| {
        let mut fields = Vec::with_capacity(64);
        for i in 0..64u32 {
            let h = match i % 8 {
                0 => b.bool(),
                1 => b.u8(),
                2 => b.u16(),
                3 => b.u32(),
                4 => b.u64(),
                5 => b.u128(),
                6 => b.u256(),
                _ => b.address(),
            };
            fields.push((ident(&format!("f{i}")), h));
        }
        b.struct_layout(st("Wide"), fields)
    })
    .unwrap()
}

fn deep_nested() -> MoveTypeLayout {
    MoveTypeLayoutBuilder::with_builder::<_, anyhow::Error>(|b| {
        let mut current: LayoutHandle = b.u64();
        for i in 0..16u32 {
            current = b.struct_layout(st(&format!("D{i}")), vec![(ident("f"), current)])?;
        }
        Ok::<_, anyhow::Error>(current)
    })
    .unwrap()
}

fn wide_enum() -> MoveTypeLayout {
    MoveTypeLayoutBuilder::with_builder::<_, anyhow::Error>(|b| {
        let mut variants = Vec::with_capacity(32);
        for i in 0..32u16 {
            let fields = vec![
                (ident("a"), b.bool()),
                (ident("b"), b.u64()),
                (ident("c"), b.address()),
                (ident("d"), b.u128()),
            ];
            variants.push((ident(&format!("V{i}")), i, Some(fields)));
        }
        b.enum_layout(st("WideEnum"), variants)
    })
    .unwrap()
}

fn realistic() -> MoveTypeLayout {
    // Outer { id: UID, balance: Balance<T>, name: vector<u8>, status: Status }
    //   UID    { id: ID { bytes: address } }
    //   Status = Active | Closed { reason: vector<u8> } | Pending { at: u64 }
    MoveTypeLayoutBuilder::with_builder::<_, anyhow::Error>(|b| {
        let addr = b.address();
        let id_inner = b.struct_layout(st("ID"), vec![(ident("bytes"), addr)])?;
        let uid = b.struct_layout(st("UID"), vec![(ident("id"), id_inner)])?;
        let value_h = b.u64();
        let balance = b.struct_layout(st("Balance"), vec![(ident("value"), value_h)])?;
        let bytes_vec = {
            let u8h = b.u8();
            b.vector(u8h)?
        };
        let bytes_vec_for_enum = {
            let u8h = b.u8();
            b.vector(u8h)?
        };
        let pending_field = b.u64();
        let status = b.enum_layout(
            st("Status"),
            vec![
                (ident("Active"), 0, Some(vec![])),
                (
                    ident("Closed"),
                    1,
                    Some(vec![(ident("reason"), bytes_vec_for_enum)]),
                ),
                (
                    ident("Pending"),
                    2,
                    Some(vec![(ident("at"), pending_field)]),
                ),
            ],
        )?;
        b.struct_layout(
            st("Outer"),
            vec![
                (ident("id"), uid),
                (ident("balance"), balance),
                (ident("name"), bytes_vec),
                (ident("status"), status),
            ],
        )
    })
    .unwrap()
}
