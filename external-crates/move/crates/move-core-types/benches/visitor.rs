// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

//! Visitor microbenchmarks: BCS-bytes-driven traversal.
//!
//! Each iter pretends to deserialize a Move value: walks bytes that match
//! the layout, calling per-byte work guided by the layout's structure.
//! This exercises the visitor framework end-to-end (cursor + driver +
//! visitor dispatch) — i.e. the cost a real `MoveValue::visit_deserialize`
//! caller pays.
//!
//! ## Variants under test
//!
//! Two benchmark groups: `annotated_traversal` and `runtime_traversal`.
//! Each group iterates the six shapes from [`common::SHAPE_NAMES`].
//!
//! ### `annotated_traversal/<shape>/<variant>`
//!
//! | Variant      | Path                                                                      | Purpose                                              |
//! |--------------|---------------------------------------------------------------------------|------------------------------------------------------|
//! | `owned`      | `MoveValue::visit_deserialize` over an owned [`MoveTypeLayout`]           | Production-style owned visitor                       |
//! | `ref`        | `annotated_visitor_ref::visit_value` over a borrowed `MoveTypeLayoutRef`  | Same trait shape, borrowed layout                    |
//! | `ref_walk`   | Hand-rolled recursive byte walker over the ref layout (no Visitor trait)  | Isolates layout-machinery cost from trait dispatch   |
//! | `exp_walk`   | Hand-rolled recursive byte walker over the experimental Exp layout         | Same workload against the alternative pool design    |
//!
//! Comparing `ref` vs. `ref_walk` measures the Visitor framework's per-step
//! overhead. Comparing `ref_walk` vs. `exp_walk` is the apples-to-apples
//! "which pool layout traverses faster" measurement.
//!
//! ### `runtime_traversal/<shape>/<variant>`
//!
//! | Variant | Path                                                                  |
//! |---------|-----------------------------------------------------------------------|
//! | `owned` | `runtime_value::MoveValue::visit_deserialize` over the runtime layout |
//! | `ref`   | `runtime_visitor_ref::visit_value` over the borrowed runtime layout   |
//!
//! Run with `cargo bench --bench visitor`.

mod common;

use std::io::Cursor;

use criterion::{Criterion, black_box, criterion_group, criterion_main};
use move_core_types::{
    annotated_value::MoveValue as AnnotatedMoveValue,
    annotated_visitor::NullTraversal as AnnotatedOwnedNullTraversal,
    annotated_visitor_ref::{
        NullTraversal as AnnotatedRefNullTraversal, visit_value as annotated_ref_visit_value,
    },
    compressed::annotated::{
        ExpMoveTypeLayout, ExpMoveTypeLayoutRef, MoveLayoutViewRef, MoveTypeLayout,
    },
    compressed::runtime::{
        LayoutHandle as RtLayoutHandle, MoveTypeLayout as RtMoveTypeLayout,
        MoveTypeLayoutBuilder as RtMoveTypeLayoutBuilder,
    },
    runtime_value::MoveValue as RuntimeMoveValue,
    runtime_visitor::NullTraversal as RuntimeOwnedNullTraversal,
    runtime_visitor_ref::{
        NullTraversal as RuntimeRefNullTraversal, visit_value as runtime_ref_visit_value,
    },
};

use crate::common::{SHAPE_NAMES, annotated_layout};

// ---------------------------------------------------------------------------
// Runtime (untyped) layout shapes — same structures as `common::annotated_layout`
// without names or `StructTag`s. Lives here because no other bench uses them.
// ---------------------------------------------------------------------------

fn runtime_layout(name: &str) -> RtMoveTypeLayout {
    match name {
        "leaf" => rt_leaf(),
        "shallow_struct" => rt_shallow_struct(),
        "wide_struct" => rt_wide_struct(),
        "deep_nested" => rt_deep_nested(),
        "wide_enum" => rt_wide_enum(),
        "realistic" => rt_realistic(),
        _ => panic!("unknown shape: {name}"),
    }
}

fn rt_leaf() -> RtMoveTypeLayout {
    RtMoveTypeLayout::u64()
}

fn rt_shallow_struct() -> RtMoveTypeLayout {
    RtMoveTypeLayoutBuilder::with_builder::<_, anyhow::Error>(|b| {
        let fields = [
            b.bool(),
            b.u8(),
            b.u16(),
            b.u32(),
            b.u64(),
            b.u128(),
            b.address(),
            b.u256(),
        ];
        b.struct_layout(&fields)
    })
    .unwrap()
}

fn rt_wide_struct() -> RtMoveTypeLayout {
    RtMoveTypeLayoutBuilder::with_builder::<_, anyhow::Error>(|b| {
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
            fields.push(h);
        }
        b.struct_layout(&fields)
    })
    .unwrap()
}

fn rt_deep_nested() -> RtMoveTypeLayout {
    RtMoveTypeLayoutBuilder::with_builder::<_, anyhow::Error>(|b| {
        let mut current: RtLayoutHandle = b.u64();
        for _ in 0..16u32 {
            current = b.struct_layout(&[current])?;
        }
        Ok::<_, anyhow::Error>(current)
    })
    .unwrap()
}

fn rt_wide_enum() -> RtMoveTypeLayout {
    RtMoveTypeLayoutBuilder::with_builder::<_, anyhow::Error>(|b| {
        let mut variants = Vec::with_capacity(32);
        for _ in 0..32u16 {
            variants.push(Some(vec![b.bool(), b.u64(), b.address(), b.u128()]));
        }
        b.enum_layout(variants)
    })
    .unwrap()
}

fn rt_realistic() -> RtMoveTypeLayout {
    RtMoveTypeLayoutBuilder::with_builder::<_, anyhow::Error>(|b| {
        let addr = b.address();
        let id_inner = b.struct_layout(&[addr])?;
        let uid = b.struct_layout(&[id_inner])?;
        let value_h = b.u64();
        let balance = b.struct_layout(&[value_h])?;
        let bytes_vec = {
            let u8h = b.u8();
            b.vector(u8h)?
        };
        let bytes_vec_for_enum = {
            let u8h = b.u8();
            b.vector(u8h)?
        };
        let pending_field = b.u64();
        let status = b.enum_layout(vec![
            Some(vec![]),
            Some(vec![bytes_vec_for_enum]),
            Some(vec![pending_field]),
        ])?;
        b.struct_layout(&[uid, balance, bytes_vec, status])
    })
    .unwrap()
}

// ---------------------------------------------------------------------------
// BCS bytes that decode against the layouts above.
//
// All-zero bytes happen to be a valid BCS encoding for every shape used here:
//   * primitives: zero-valued instances are valid;
//   * `vector<u8>` in `realistic`: leading `0x00` is `leb128(0)` = empty vector;
//   * enums: tag `0` selects variant 0. For `realistic`'s `Status` enum that's
//     `Active` (no fields). For `wide_enum`, variant 0 has 4 primitive fields
//     whose zero encodings follow.
// ---------------------------------------------------------------------------

fn bytes_for(name: &str) -> Vec<u8> {
    match name {
        // u64 = 8 bytes
        "leaf" => vec![0u8; 8],
        // bool(1) + u8(1) + u16(2) + u32(4) + u64(8) + u128(16) + addr(32) + u256(32) = 96
        "shallow_struct" => vec![0u8; 96],
        // 8 cycles × 96 bytes per cycle = 768
        "wide_struct" => vec![0u8; 96 * 8],
        // 16 nested transparent structs → final u64 = 8 bytes
        "deep_nested" => vec![0u8; 8],
        // tag(1) + bool(1) + u64(8) + addr(32) + u128(16) = 58
        "wide_enum" => vec![0u8; 58],
        // uid(32) + balance(8) + vector<u8> empty(1) + enum tag 0 (Active)(1) = 42
        "realistic" => vec![0u8; 42],
        _ => panic!("unknown shape: {name}"),
    }
}

// ---------------------------------------------------------------------------
// Visitor-framework drivers (exercise full Visitor dispatch).
// ---------------------------------------------------------------------------

fn run_annotated_owned(bytes: &[u8], layout: &MoveTypeLayout) {
    AnnotatedMoveValue::visit_deserialize(bytes, layout.clone(), &mut AnnotatedOwnedNullTraversal)
        .unwrap()
}

fn run_annotated_ref(bytes: &[u8], layout: &MoveTypeLayout) {
    let mut cursor = Cursor::new(bytes);
    annotated_ref_visit_value(
        &mut cursor,
        layout.as_layout_ref(),
        &mut AnnotatedRefNullTraversal,
    )
    .unwrap();
}

fn run_runtime_owned(bytes: &[u8], layout: &RtMoveTypeLayout) {
    RuntimeMoveValue::visit_deserialize(bytes, layout.clone(), &mut RuntimeOwnedNullTraversal)
        .unwrap()
}

fn run_runtime_ref(bytes: &[u8], layout: &RtMoveTypeLayout) {
    let mut cursor = Cursor::new(bytes);
    runtime_ref_visit_value(
        &mut cursor,
        layout.as_layout_ref(),
        &mut RuntimeRefNullTraversal,
    )
    .unwrap();
}

// ---------------------------------------------------------------------------
// Hand-rolled byte walkers (no Visitor trait, no NullTraversal indirection).
//
// Same semantic as a NullTraversal-driven visit (read all bytes, advance the
// cursor) but written as a direct recursion. Lets us isolate the cost of
// the layout machinery itself from the cost of the Visitor framework's
// trait dispatch.
// ---------------------------------------------------------------------------

#[derive(thiserror::Error, Debug)]
enum WalkError {
    #[error("unexpected byte: {0}")]
    UnexpectedByte(u8),
    #[error("unexpected eof")]
    UnexpectedEof,
    #[error("invalid variant tag: {0}")]
    BadTag(u16),
    #[error("unknown variant layout")]
    UnknownVariant,
}

fn read_n<const N: usize>(c: &mut Cursor<&[u8]>) -> Result<(), WalkError> {
    use std::io::Read;
    let mut buf = [0u8; N];
    c.read_exact(&mut buf)
        .map_err(|_| WalkError::UnexpectedEof)?;
    Ok(())
}

fn read_one(c: &mut Cursor<&[u8]>) -> Result<u8, WalkError> {
    use std::io::Read;
    let mut buf = [0u8; 1];
    c.read_exact(&mut buf)
        .map_err(|_| WalkError::UnexpectedEof)?;
    Ok(buf[0])
}

fn read_leb128(c: &mut Cursor<&[u8]>) -> Result<u64, WalkError> {
    leb128::read::unsigned(c).map_err(|_| WalkError::UnexpectedEof)
}

/// Walk BCS bytes guided by a compressed annotated *ref* layout.
fn walk_annotated_ref_view(
    c: &mut Cursor<&[u8]>,
    view: MoveLayoutViewRef<'_>,
) -> Result<(), WalkError> {
    match view {
        MoveLayoutViewRef::Bool => match read_one(c)? {
            0 | 1 => Ok(()),
            b => Err(WalkError::UnexpectedByte(b)),
        },
        MoveLayoutViewRef::U8 => read_n::<1>(c),
        MoveLayoutViewRef::U16 => read_n::<2>(c),
        MoveLayoutViewRef::U32 => read_n::<4>(c),
        MoveLayoutViewRef::U64 => read_n::<8>(c),
        MoveLayoutViewRef::U128 => read_n::<16>(c),
        MoveLayoutViewRef::U256 => read_n::<32>(c),
        MoveLayoutViewRef::Address => read_n::<32>(c),
        MoveLayoutViewRef::Signer => read_n::<32>(c),
        MoveLayoutViewRef::Vector(inner) => {
            let n = read_leb128(c)?;
            for _ in 0..n {
                walk_annotated_ref_view(c, inner.as_view())?;
            }
            Ok(())
        }
        MoveLayoutViewRef::Struct(s) => {
            for (_, sub) in s.fields() {
                walk_annotated_ref_view(c, sub.as_view())?;
            }
            Ok(())
        }
        MoveLayoutViewRef::Enum(e) => {
            let tag = read_one(c)? as u16;
            let v = e.variant_by_tag(tag).ok_or(WalkError::BadTag(tag))?;
            let fs = v.fields().ok_or(WalkError::UnknownVariant)?;
            for (_, sub) in fs.fields() {
                walk_annotated_ref_view(c, sub.as_view())?;
            }
            Ok(())
        }
    }
}

fn run_annotated_walk(bytes: &[u8], layout: &MoveTypeLayout) {
    let mut c = Cursor::new(bytes);
    walk_annotated_ref_view(&mut c, layout.as_view_ref()).unwrap()
}

/// Walk BCS bytes guided by an Exp layout ref.
fn walk_exp_view(c: &mut Cursor<&[u8]>, view: ExpMoveTypeLayoutRef<'_>) -> Result<(), WalkError> {
    match view {
        ExpMoveTypeLayoutRef::Bool => match read_one(c)? {
            0 | 1 => Ok(()),
            b => Err(WalkError::UnexpectedByte(b)),
        },
        ExpMoveTypeLayoutRef::U8 => read_n::<1>(c),
        ExpMoveTypeLayoutRef::U16 => read_n::<2>(c),
        ExpMoveTypeLayoutRef::U32 => read_n::<4>(c),
        ExpMoveTypeLayoutRef::U64 => read_n::<8>(c),
        ExpMoveTypeLayoutRef::U128 => read_n::<16>(c),
        ExpMoveTypeLayoutRef::U256 => read_n::<32>(c),
        ExpMoveTypeLayoutRef::Address => read_n::<32>(c),
        ExpMoveTypeLayoutRef::Signer => read_n::<32>(c),
        ExpMoveTypeLayoutRef::Vector(v) => {
            let n = read_leb128(c)?;
            let elem = v.element();
            for _ in 0..n {
                walk_exp_view(c, elem)?;
            }
            Ok(())
        }
        ExpMoveTypeLayoutRef::Struct(s) => {
            for fld in s.fields() {
                walk_exp_view(c, fld.layout())?;
            }
            Ok(())
        }
        ExpMoveTypeLayoutRef::Enum(e) => {
            let tag = read_one(c)? as u16;
            let v = e.variant_by_tag(tag).ok_or(WalkError::BadTag(tag))?;
            let fs = v.fields().ok_or(WalkError::UnknownVariant)?;
            for fld in fs {
                walk_exp_view(c, fld.layout())?;
            }
            Ok(())
        }
    }
}

fn run_exp_walk(bytes: &[u8], layout: &ExpMoveTypeLayout) {
    let mut c = Cursor::new(bytes);
    walk_exp_view(&mut c, layout.as_layout_ref()).unwrap()
}

// ---------------------------------------------------------------------------
// Bench drivers
// ---------------------------------------------------------------------------

fn bench_annotated(c: &mut Criterion) {
    let mut group = c.benchmark_group("annotated_traversal");
    for &name in SHAPE_NAMES {
        let layout = annotated_layout(name);
        let bytes = bytes_for(name);
        let exp = ExpMoveTypeLayout::try_from(&layout.inflate().unwrap()).unwrap();

        // Sanity-check before timing: every variant should successfully
        // walk the buffer without panicking.
        run_annotated_owned(&bytes, &layout);
        run_annotated_ref(&bytes, &layout);
        run_annotated_walk(&bytes, &layout);
        run_exp_walk(&bytes, &exp);

        group.bench_function(format!("{name}/owned"), |b| {
            b.iter(|| run_annotated_owned(black_box(&bytes), black_box(&layout)))
        });
        group.bench_function(format!("{name}/ref"), |b| {
            b.iter(|| run_annotated_ref(black_box(&bytes), black_box(&layout)))
        });
        group.bench_function(format!("{name}/ref_walk"), |b| {
            b.iter(|| run_annotated_walk(black_box(&bytes), black_box(&layout)))
        });
        group.bench_function(format!("{name}/exp_walk"), |b| {
            b.iter(|| run_exp_walk(black_box(&bytes), black_box(&exp)))
        });
    }
    group.finish();
}

fn bench_runtime(c: &mut Criterion) {
    let mut group = c.benchmark_group("runtime_traversal");
    for &name in SHAPE_NAMES {
        let layout = runtime_layout(name);
        let bytes = bytes_for(name);

        // Sanity-check before timing.
        run_runtime_owned(&bytes, &layout);
        run_runtime_ref(&bytes, &layout);

        group.bench_function(format!("{name}/owned"), |b| {
            b.iter(|| run_runtime_owned(black_box(&bytes), black_box(&layout)))
        });
        group.bench_function(format!("{name}/ref"), |b| {
            b.iter(|| run_runtime_ref(black_box(&bytes), black_box(&layout)))
        });
    }
    group.finish();
}

criterion_group!(benches, bench_annotated, bench_runtime);
criterion_main!(benches);
