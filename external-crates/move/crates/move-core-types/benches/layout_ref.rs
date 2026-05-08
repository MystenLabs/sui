// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

//! Layout-only microbenchmarks: traversal, lookup, and clone, comparing
//! three representations of the compressed annotated layout.
//!
//! ## Variants under test
//!
//! - **`owned`** — [`MoveTypeLayout`] / [`MoveLayoutView`] from
//!   [`compressed::annotated`]. Each `as_view()` produces an owned view
//!   that holds `Arc`-shared pool, `StructTag`, and `Identifier` data.
//! - **`ref`** — [`MoveTypeLayoutRef`] / [`MoveLayoutViewRef`] from
//!   [`compressed::annotated::ref_layout`]. All `Copy`, alloc-free.
//! - **`exp`** — `Exp*` types from [`compressed::annotated::exp_layout`].
//!   Alternative pool representation (struct-of-vecs); always traversed in
//!   borrowed form.
//!
//! ## Workloads
//!
//! Three benchmark groups, each running over the six shapes catalogued in
//! [`common::SHAPE_NAMES`]:
//!
//! | Group       | What it does                                                                                | Why                                              |
//! |-------------|---------------------------------------------------------------------------------------------|--------------------------------------------------|
//! | `traversal` | Walks every reachable node, accumulating an invariant (sum of name lengths + leaf count).   | Measures full DAG-walk cost, no I/O.             |
//! | `lookup`    | Repeats `field_by_name` chains 100× per iter, following dotted paths.                       | Measures random-access path resolution.          |
//! | `clone`     | `clone()` (owned) vs. `Copy` (ref) of the layout root.                                      | Isolates the cost of duplicating a layout handle. |
//!
//! Run with `cargo bench --bench layout_ref`.

mod common;

use criterion::{Criterion, black_box, criterion_group, criterion_main};
use move_core_types::compressed::annotated::{
    ExpMoveTypeLayout, ExpMoveTypeLayoutRef, MoveLayoutViewRef, MoveTypeLayout,
};

use crate::common::{SHAPE_NAMES, annotated_layout};

// ---------------------------------------------------------------------------
// Shape preparation
// ---------------------------------------------------------------------------

fn shapes() -> Vec<(&'static str, MoveTypeLayout)> {
    SHAPE_NAMES
        .iter()
        .map(|&name| (name, annotated_layout(name)))
        .collect()
}

/// Convert each annotated layout to its experimental counterpart by
/// inflating to tree form and back. Done once outside any timed region.
fn shapes_exp() -> Vec<(&'static str, ExpMoveTypeLayout)> {
    shapes()
        .into_iter()
        .map(|(name, layout)| {
            let tree = layout.inflate().expect("inflate");
            let exp = ExpMoveTypeLayout::try_from(&tree).expect("exp build");
            (name, exp)
        })
        .collect()
}

// ---------------------------------------------------------------------------
// W1 — full traversal
// ---------------------------------------------------------------------------
//
// Each function recursively visits every node and returns `usize` so the
// optimizer can't elide the work. The accumulator is identical across the
// three variants (sum of name string lengths plus a leaf count) so any
// runtime delta is attributable to the layout machinery.

fn traverse_owned(layout: &MoveTypeLayout) -> usize {
    use move_core_types::compressed::annotated::MoveLayoutView as V;

    fn go(v: &V) -> usize {
        match v {
            V::Bool
            | V::U8
            | V::U16
            | V::U32
            | V::U64
            | V::U128
            | V::U256
            | V::Address
            | V::Signer => 1,
            V::Vector(inner) => 1 + go(&inner.as_view()),
            V::Struct(s) => {
                let mut acc = s.type_().name.as_str().len();
                for (name, sub) in s.fields() {
                    acc += name.as_str().len();
                    acc += go(&sub.as_view());
                }
                acc
            }
            V::Enum(e) => {
                let mut acc = e.type_().name.as_str().len();
                for vl in e.variants() {
                    acc += vl.name().as_str().len();
                    if let Some(fs) = vl.fields() {
                        for (name, sub) in fs.fields() {
                            acc += name.as_str().len();
                            acc += go(&sub.as_view());
                        }
                    }
                }
                acc
            }
        }
    }
    go(&layout.as_view())
}

fn traverse_ref(layout: &MoveTypeLayout) -> usize {
    fn go(v: MoveLayoutViewRef<'_>) -> usize {
        match v {
            MoveLayoutViewRef::Bool
            | MoveLayoutViewRef::U8
            | MoveLayoutViewRef::U16
            | MoveLayoutViewRef::U32
            | MoveLayoutViewRef::U64
            | MoveLayoutViewRef::U128
            | MoveLayoutViewRef::U256
            | MoveLayoutViewRef::Address
            | MoveLayoutViewRef::Signer => 1,
            MoveLayoutViewRef::Vector(inner) => 1 + go(inner.as_view()),
            MoveLayoutViewRef::Struct(s) => {
                let mut acc = s.type_().name.as_str().len();
                for (name, sub) in s.fields() {
                    acc += name.as_str().len();
                    acc += go(sub.as_view());
                }
                acc
            }
            MoveLayoutViewRef::Enum(e) => {
                let mut acc = e.type_().name.as_str().len();
                for vl in e.variants() {
                    acc += vl.name().as_str().len();
                    if let Some(fs) = vl.fields() {
                        for (name, sub) in fs.fields() {
                            acc += name.as_str().len();
                            acc += go(sub.as_view());
                        }
                    }
                }
                acc
            }
        }
    }
    go(layout.as_view_ref())
}

fn traverse_exp(layout: &ExpMoveTypeLayout) -> usize {
    fn go(v: ExpMoveTypeLayoutRef<'_>) -> usize {
        match v {
            ExpMoveTypeLayoutRef::Bool
            | ExpMoveTypeLayoutRef::U8
            | ExpMoveTypeLayoutRef::U16
            | ExpMoveTypeLayoutRef::U32
            | ExpMoveTypeLayoutRef::U64
            | ExpMoveTypeLayoutRef::U128
            | ExpMoveTypeLayoutRef::U256
            | ExpMoveTypeLayoutRef::Address
            | ExpMoveTypeLayoutRef::Signer => 1,
            ExpMoveTypeLayoutRef::Vector(v) => 1 + go(v.element()),
            ExpMoveTypeLayoutRef::Struct(s) => {
                let mut acc = s.type_().name.as_str().len();
                for fld in s.fields() {
                    acc += fld.name().as_str().len();
                    acc += go(fld.layout());
                }
                acc
            }
            ExpMoveTypeLayoutRef::Enum(e) => {
                let mut acc = e.type_().name.as_str().len();
                for vl in e.variants() {
                    acc += vl.name().as_str().len();
                    if let Some(fs) = vl.fields() {
                        for fld in fs {
                            acc += fld.name().as_str().len();
                            acc += go(fld.layout());
                        }
                    }
                }
                acc
            }
        }
    }
    go(layout.as_layout_ref())
}

// ---------------------------------------------------------------------------
// W2 — random-access lookup
// ---------------------------------------------------------------------------
//
// Each call resolves a dotted path against a layout root, descending through
// `field_by_name` at each segment. Empty paths probe `as_view()` /
// `as_layout_ref()` only — useful for the wide_enum shape, where
// `as_view()` itself is the meaningful workload (in the optimized owned
// representation, materializing the enum's variants is now lazy).
//
// Returns the number of segments successfully resolved, so the optimizer
// can't elide the descent.

fn lookup_paths_for(name: &str) -> Vec<&'static str> {
    match name {
        "leaf" => vec![""],
        "shallow_struct" => vec!["f0", "f4", "f7", "fX"],
        "wide_struct" => vec!["f0", "f17", "f63", "fX"],
        "deep_nested" => vec!["f", "f.f", "f.f.f.f.f.f.f.f.f.f.f.f.f.f.f"],
        "wide_enum" => vec![""],
        "realistic" => vec![
            "id.id.bytes",
            "balance.value",
            "name",
            "status",
            "id.id.missing",
        ],
        _ => unreachable!("unknown shape: {name}"),
    }
}

fn lookup_owned(layout: &MoveTypeLayout, path: &str) -> usize {
    use move_core_types::compressed::annotated::MoveLayoutView as V;
    if path.is_empty() {
        return matches!(layout.as_view(), V::Bool) as usize;
    }
    let mut current = layout.clone();
    let mut hit = 0usize;
    for seg in path.split('.') {
        let view = current.as_view();
        match view {
            V::Struct(s) => {
                if let Some(next) = s.fields_layout().field_by_name(seg) {
                    current = next;
                    hit += 1;
                } else {
                    break;
                }
            }
            _ => break,
        }
    }
    hit
}

fn lookup_ref(layout: &MoveTypeLayout, path: &str) -> usize {
    if path.is_empty() {
        return matches!(layout.as_view_ref(), MoveLayoutViewRef::Bool) as usize;
    }
    let mut current = layout.as_layout_ref();
    let mut hit = 0usize;
    for seg in path.split('.') {
        match current.as_view() {
            MoveLayoutViewRef::Struct(s) => {
                if let Some(next) = s.fields_layout().field_by_name(seg) {
                    current = next;
                    hit += 1;
                } else {
                    break;
                }
            }
            _ => break,
        }
    }
    hit
}

fn lookup_exp(layout: &ExpMoveTypeLayout, path: &str) -> usize {
    if path.is_empty() {
        return matches!(layout.as_layout_ref(), ExpMoveTypeLayoutRef::Bool) as usize;
    }
    let mut current = layout.as_layout_ref();
    let mut hit = 0usize;
    for seg in path.split('.') {
        match current {
            ExpMoveTypeLayoutRef::Struct(s) => match s.field_by_name(seg) {
                Some(f) => {
                    current = f.layout();
                    hit += 1;
                }
                None => break,
            },
            _ => break,
        }
    }
    hit
}

// ---------------------------------------------------------------------------
// Bench drivers
// ---------------------------------------------------------------------------

/// Bench-name format for each function: `<shape>/<variant>`. Criterion
/// groups them by shape in its output.
fn bench_traversal(c: &mut Criterion) {
    let mut group = c.benchmark_group("traversal");
    let exp = shapes_exp();
    for ((name, layout), (_, exp_layout)) in shapes().into_iter().zip(exp.iter()) {
        group.bench_function(format!("{name}/owned"), |b| {
            b.iter(|| black_box(traverse_owned(black_box(&layout))))
        });
        group.bench_function(format!("{name}/ref"), |b| {
            b.iter(|| black_box(traverse_ref(black_box(&layout))))
        });
        group.bench_function(format!("{name}/exp"), |b| {
            b.iter(|| black_box(traverse_exp(black_box(exp_layout))))
        });
    }
    group.finish();
}

fn bench_lookup(c: &mut Criterion) {
    let mut group = c.benchmark_group("lookup");
    let exp = shapes_exp();
    for ((name, layout), (_, exp_layout)) in shapes().into_iter().zip(exp.iter()) {
        let paths = lookup_paths_for(name);
        group.bench_function(format!("{name}/owned"), |b| {
            b.iter(|| {
                let mut acc = 0usize;
                for _ in 0..100 {
                    for p in &paths {
                        acc += lookup_owned(black_box(&layout), black_box(p));
                    }
                }
                black_box(acc)
            })
        });
        group.bench_function(format!("{name}/ref"), |b| {
            b.iter(|| {
                let mut acc = 0usize;
                for _ in 0..100 {
                    for p in &paths {
                        acc += lookup_ref(black_box(&layout), black_box(p));
                    }
                }
                black_box(acc)
            })
        });
        group.bench_function(format!("{name}/exp"), |b| {
            b.iter(|| {
                let mut acc = 0usize;
                for _ in 0..100 {
                    for p in &paths {
                        acc += lookup_exp(black_box(exp_layout), black_box(p));
                    }
                }
                black_box(acc)
            })
        });
    }
    group.finish();
}

/// `clone` runs four sub-variants per shape: owned vs. ref for the
/// canonical family, and `clone` vs. ref-borrow for the experimental
/// family.
fn bench_clone(c: &mut Criterion) {
    let mut group = c.benchmark_group("clone");
    let exp = shapes_exp();
    for ((name, layout), (_, exp_layout)) in shapes().into_iter().zip(exp.iter()) {
        group.bench_function(format!("{name}/owned"), |b| {
            b.iter(|| black_box(black_box(&layout).clone()))
        });
        group.bench_function(format!("{name}/ref"), |b| {
            b.iter(|| {
                let r = black_box(&layout).as_layout_ref();
                black_box(r)
            })
        });
        group.bench_function(format!("{name}/exp_clone"), |b| {
            b.iter(|| black_box(black_box(exp_layout).clone()))
        });
        group.bench_function(format!("{name}/exp_ref"), |b| {
            b.iter(|| {
                let r = black_box(exp_layout).as_layout_ref();
                black_box(r)
            })
        });
    }
    group.finish();
}

criterion_group!(benches, bench_traversal, bench_lookup, bench_clone);
criterion_main!(benches);
