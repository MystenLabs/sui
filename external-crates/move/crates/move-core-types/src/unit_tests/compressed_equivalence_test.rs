// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

//! Tests for `equivalent` on compressed type layouts (runtime + annotated).
//!
//! Equivalence is **structural**: two layouts compare equal iff they describe
//! the same Move type, regardless of pool ordering or how subtrees are shared.

use crate::{
    annotated_value as A,
    compressed::{
        annotated::{self as CA, BackendBuilder as _},
        runtime::{self as CR, BackendBuilder as _},
    },
    identifier::Identifier,
    language_storage::StructTag,
    runtime_value as R,
};
use rand::{Rng, SeedableRng, rngs::StdRng, seq::SliceRandom};
use std::collections::BTreeMap;
use std::str::FromStr;

// =============================================================================
// Helpers: build compressed layouts by interning a tree in a shuffled visit
// order. This produces the same logical layout but with different pool indices.
// =============================================================================

fn compress_runtime(t: &R::MoveTypeLayout) -> CR::MoveTypeLayout {
    t.try_into().unwrap()
}

fn compress_annotated(t: &A::MoveTypeLayout) -> CA::MoveTypeLayout {
    t.try_into().unwrap()
}

/// Build a compressed runtime layout from a tree, interning children in a
/// permuted order driven by `seed`. The resulting layout is logically the same
/// as `compress_runtime(t)` but the pool indices typically differ.
fn compress_with_shuffle_runtime(t: &R::MoveTypeLayout, seed: u64) -> CR::MoveTypeLayout {
    let mut b = CR::MoveTypeLayoutBuilder::new();
    let mut rng = StdRng::seed_from_u64(seed);
    let root = intern_shuffled_runtime(&mut b, t, &mut rng);
    b.build(root)
}

fn intern_shuffled_runtime(
    b: &mut CR::MoveTypeLayoutBuilder,
    t: &R::MoveTypeLayout,
    rng: &mut StdRng,
) -> CR::LayoutHandle {
    use R::MoveTypeLayout as T;
    match t {
        T::Bool => b.bool(),
        T::U8 => b.u8(),
        T::U16 => b.u16(),
        T::U32 => b.u32(),
        T::U64 => b.u64(),
        T::U128 => b.u128(),
        T::U256 => b.u256(),
        T::Address => b.address(),
        T::Signer => b.signer(),
        T::Vector(inner) => {
            let h = intern_shuffled_runtime(b, inner, rng);
            b.vector(h).unwrap()
        }
        T::Struct(s) => {
            let mut order: Vec<usize> = (0..s.0.len()).collect();
            order.shuffle(rng);
            let mut handles: Vec<Option<CR::LayoutHandle>> = (0..s.0.len()).map(|_| None).collect();
            for i in order {
                handles[i] = Some(intern_shuffled_runtime(b, &s.0[i], rng));
            }
            let handles: Vec<CR::LayoutHandle> = handles.into_iter().map(Option::unwrap).collect();
            b.struct_layout(&handles).unwrap()
        }
        T::Enum(e) => {
            let mut handles: Vec<Option<Vec<CR::LayoutHandle>>> =
                (0..e.0.len()).map(|_| None).collect();
            let mut order: Vec<usize> = (0..e.0.len()).collect();
            order.shuffle(rng);
            for i in order {
                let variant = &e.0[i];
                let mut field_order: Vec<usize> = (0..variant.len()).collect();
                field_order.shuffle(rng);
                let mut field_handles: Vec<Option<CR::LayoutHandle>> =
                    (0..variant.len()).map(|_| None).collect();
                for j in field_order {
                    field_handles[j] = Some(intern_shuffled_runtime(b, &variant[j], rng));
                }
                handles[i] = Some(field_handles.into_iter().map(Option::unwrap).collect());
            }
            let variants: Vec<Vec<CR::LayoutHandle>> =
                handles.into_iter().map(Option::unwrap).collect();
            let variant_refs: Vec<Option<&[CR::LayoutHandle]>> =
                variants.iter().map(|v| Some(v.as_slice())).collect();
            b.enum_layout(&variant_refs).unwrap()
        }
    }
}

/// Build a compressed annotated layout from a tree, interning children in a
/// permuted order driven by `seed`.
fn compress_with_shuffle_annotated(t: &A::MoveTypeLayout, seed: u64) -> CA::MoveTypeLayout {
    let mut b = CA::MoveTypeLayoutBuilder::new();
    let mut rng = StdRng::seed_from_u64(seed);
    let root = intern_shuffled_annotated(&mut b, t, &mut rng);
    b.build(root)
}

fn intern_shuffled_annotated(
    b: &mut CA::MoveTypeLayoutBuilder,
    t: &A::MoveTypeLayout,
    rng: &mut StdRng,
) -> CA::LayoutHandle {
    use A::MoveTypeLayout as T;
    match t {
        T::Bool => b.bool(),
        T::U8 => b.u8(),
        T::U16 => b.u16(),
        T::U32 => b.u32(),
        T::U64 => b.u64(),
        T::U128 => b.u128(),
        T::U256 => b.u256(),
        T::Address => b.address(),
        T::Signer => b.signer(),
        T::Vector(inner) => {
            let h = intern_shuffled_annotated(b, inner, rng);
            b.vector(h).unwrap()
        }
        T::Struct(s) => {
            let n = s.fields.len();
            let mut order: Vec<usize> = (0..n).collect();
            order.shuffle(rng);
            let mut handles: Vec<Option<(Identifier, CA::LayoutHandle)>> =
                (0..n).map(|_| None).collect();
            for i in order {
                let f = &s.fields[i];
                let h = intern_shuffled_annotated(b, &f.layout, rng);
                handles[i] = Some((f.name.clone(), h));
            }
            let handles: Vec<(Identifier, CA::LayoutHandle)> =
                handles.into_iter().map(Option::unwrap).collect();
            let handle_refs: Vec<(&Identifier, CA::LayoutHandle)> =
                handles.iter().map(|(n, h)| (n, *h)).collect();
            b.struct_layout(&s.type_, &handle_refs).unwrap()
        }
        T::Enum(e) => {
            let entries: Vec<(&(Identifier, u16), &Vec<A::MoveFieldLayout>)> =
                e.variants.iter().collect();
            let mut order: Vec<usize> = (0..entries.len()).collect();
            order.shuffle(rng);
            let mut variants: Vec<
                Option<(Identifier, u16, Option<Vec<(Identifier, CA::LayoutHandle)>>)>,
            > = (0..entries.len()).map(|_| None).collect();
            for i in order {
                let ((vname, vtag), fields) = entries[i];
                let m = fields.len();
                let mut field_order: Vec<usize> = (0..m).collect();
                field_order.shuffle(rng);
                let mut field_handles: Vec<Option<(Identifier, CA::LayoutHandle)>> =
                    (0..m).map(|_| None).collect();
                for j in field_order {
                    let f = &fields[j];
                    let h = intern_shuffled_annotated(b, &f.layout, rng);
                    field_handles[j] = Some((f.name.clone(), h));
                }
                let field_handles: Vec<(Identifier, CA::LayoutHandle)> =
                    field_handles.into_iter().map(Option::unwrap).collect();
                variants[i] = Some((vname.clone(), *vtag, Some(field_handles)));
            }
            let variants: Vec<(Identifier, u16, Vec<(Identifier, CA::LayoutHandle)>)> = variants
                .into_iter()
                .map(|v| {
                    let (n, t, fs) = v.unwrap();
                    (n, t, fs.unwrap())
                })
                .collect();
            let variant_field_refs: Vec<Vec<(&Identifier, CA::LayoutHandle)>> = variants
                .iter()
                .map(|(_, _, fs)| fs.iter().map(|(n, h)| (n, *h)).collect())
                .collect();
            let variant_refs: Vec<(&Identifier, u16, Option<&[(&Identifier, CA::LayoutHandle)]>)> =
                variants
                    .iter()
                    .zip(variant_field_refs.iter())
                    .map(|((name, tag, _), fs)| (name, *tag, Some(fs.as_slice())))
                    .collect();
            b.enum_layout(&e.type_, &variant_refs).unwrap()
        }
    }
}

// =============================================================================
// Helpers: small layout constructors for unit tests.
// =============================================================================

fn struct_tag(rep: &str) -> StructTag {
    StructTag::from_str(rep).unwrap()
}

fn ident(s: &str) -> Identifier {
    Identifier::new(s).unwrap()
}

// =============================================================================
// Runtime: unit tests
// =============================================================================

#[test]
fn runtime_reflexive() {
    let t = R::MoveTypeLayout::Struct(Box::new(R::MoveStructLayout(Box::new(vec![
        R::MoveTypeLayout::U8,
        R::MoveTypeLayout::Vector(Box::new(R::MoveTypeLayout::U64)),
    ]))));
    let c = compress_runtime(&t);
    assert!(c.equivalent(&c));
}

#[test]
fn runtime_clone_fast_path() {
    let t = R::MoveTypeLayout::Vector(Box::new(R::MoveTypeLayout::U8));
    let a = compress_runtime(&t);
    let b = a.clone();
    // Clone shares the Arc; the fast path should fire.
    assert!(a.equivalent(&b));
}

#[test]
fn runtime_pool_permutation_invariant() {
    let t = R::MoveTypeLayout::Struct(Box::new(R::MoveStructLayout(Box::new(vec![
        R::MoveTypeLayout::Vector(Box::new(R::MoveTypeLayout::U8)),
        R::MoveTypeLayout::Vector(Box::new(R::MoveTypeLayout::U16)),
        R::MoveTypeLayout::Struct(Box::new(R::MoveStructLayout(Box::new(vec![
            R::MoveTypeLayout::U64,
        ])))),
    ]))));
    let a = compress_runtime(&t);
    // Try several seeds to cover different shuffle outcomes.
    for seed in 0..8u64 {
        let b = compress_with_shuffle_runtime(&t, seed);
        assert!(
            a.equivalent(&b),
            "seed {seed}: layouts should be equivalent under pool permutation"
        );
    }
}

#[test]
fn runtime_different_arity_not_equivalent() {
    let t1 = R::MoveTypeLayout::Struct(Box::new(R::MoveStructLayout(Box::new(vec![
        R::MoveTypeLayout::U8,
    ]))));
    let t2 = R::MoveTypeLayout::Struct(Box::new(R::MoveStructLayout(Box::new(vec![
        R::MoveTypeLayout::U8,
        R::MoveTypeLayout::U8,
    ]))));
    assert!(!compress_runtime(&t1).equivalent(&compress_runtime(&t2)));
}

#[test]
fn runtime_different_leaf_not_equivalent() {
    let t1 = R::MoveTypeLayout::Vector(Box::new(R::MoveTypeLayout::U8));
    let t2 = R::MoveTypeLayout::Vector(Box::new(R::MoveTypeLayout::U16));
    assert!(!compress_runtime(&t1).equivalent(&compress_runtime(&t2)));
}

#[test]
fn runtime_subtype_struct_equivalent() {
    let t = R::MoveTypeLayout::Struct(Box::new(R::MoveStructLayout(Box::new(vec![
        R::MoveTypeLayout::U64,
    ]))));
    let a = compress_runtime(&t);
    let b = compress_with_shuffle_runtime(&t, 7);

    let CR::MoveLayoutView::Struct(sa) = a.as_view() else {
        panic!()
    };
    let CR::MoveLayoutView::Struct(sb) = b.as_view() else {
        panic!()
    };
    assert!(sa.equivalent(&sb));
}

#[test]
fn runtime_subtype_view_equivalent() {
    let t = R::MoveTypeLayout::Vector(Box::new(R::MoveTypeLayout::U8));
    let a = compress_runtime(&t);
    let b = compress_with_shuffle_runtime(&t, 1);
    assert!(a.as_view().equivalent(&b.as_view()));
}

#[test]
fn runtime_view_mismatched_kinds_not_equivalent() {
    let l1 = compress_runtime(&R::MoveTypeLayout::U8);
    let l2 = compress_runtime(&R::MoveTypeLayout::U16);
    let v1 = l1.as_view();
    let v2 = l2.as_view();
    assert!(!v1.equivalent(&v2));

    let s = compress_runtime(&R::MoveTypeLayout::Struct(Box::new(R::MoveStructLayout(
        Box::new(vec![R::MoveTypeLayout::U8]),
    ))));
    let lu = compress_runtime(&R::MoveTypeLayout::U8);
    let v_struct = s.as_view();
    let v_u8 = lu.as_view();
    assert!(!v_struct.equivalent(&v_u8));
}

#[test]
fn runtime_unknown_variant_equivalence() {
    let mut b1 = CR::MoveTypeLayoutBuilder::new();
    let u8_h1 = b1.u8();
    let v1_fields = [u8_h1];
    let h1 = b1.enum_layout(&[None, Some(&v1_fields)]).unwrap();
    let l1 = b1.build(h1);

    let mut b2 = CR::MoveTypeLayoutBuilder::new();
    let u8_h2 = b2.u8();
    let v2_fields = [u8_h2];
    let h2 = b2.enum_layout(&[None, Some(&v2_fields)]).unwrap();
    let l2 = b2.build(h2);

    assert!(l1.equivalent(&l2));

    // Known with empty fields vs Unknown — not equivalent.
    let mut b3 = CR::MoveTypeLayoutBuilder::new();
    let u8_h3 = b3.u8();
    let v3_b: [CR::LayoutHandle; 1] = [u8_h3];
    let h3 = b3.enum_layout(&[Some(&[]), Some(&v3_b)]).unwrap();
    let l3 = b3.build(h3);
    assert!(!l1.equivalent(&l3));
}

#[test]
fn runtime_dag_with_shared_subtree() {
    // struct { a: vector<u8>, b: vector<u8> } — the inner vector<u8> is shared
    // when the builder dedups. Build twice with two different shuffle seeds and
    // confirm both compare equivalent.
    let inner = R::MoveTypeLayout::Vector(Box::new(R::MoveTypeLayout::U8));
    let t = R::MoveTypeLayout::Struct(Box::new(R::MoveStructLayout(Box::new(vec![
        inner.clone(),
        inner,
    ]))));
    let a = compress_with_shuffle_runtime(&t, 0);
    let b = compress_with_shuffle_runtime(&t, 100);
    assert!(a.equivalent(&b));
}

// =============================================================================
// Annotated: unit tests
// =============================================================================

#[test]
fn annotated_reflexive() {
    let t = A::MoveTypeLayout::Struct(Box::new(A::MoveStructLayout {
        type_: struct_tag("0x0::foo::Bar"),
        fields: vec![A::MoveFieldLayout::new(ident("a"), A::MoveTypeLayout::U8)],
    }));
    let c = compress_annotated(&t);
    assert!(c.equivalent(&c));
}

#[test]
fn annotated_clone_fast_path() {
    let t = A::MoveTypeLayout::Vector(Box::new(A::MoveTypeLayout::U8));
    let a = compress_annotated(&t);
    let b = a.clone();
    assert!(a.equivalent(&b));
}

#[test]
fn annotated_pool_permutation_invariant() {
    let inner = A::MoveTypeLayout::Vector(Box::new(A::MoveTypeLayout::U8));
    let t = A::MoveTypeLayout::Struct(Box::new(A::MoveStructLayout {
        type_: struct_tag("0x0::foo::Bar"),
        fields: vec![
            A::MoveFieldLayout::new(ident("a"), inner.clone()),
            A::MoveFieldLayout::new(ident("b"), inner),
        ],
    }));
    let a = compress_annotated(&t);
    for seed in 0..8u64 {
        let b = compress_with_shuffle_annotated(&t, seed);
        assert!(a.equivalent(&b), "seed {seed}");
    }
}

#[test]
fn annotated_different_struct_tag_not_equivalent() {
    let mk = |tag: &str| {
        compress_annotated(&A::MoveTypeLayout::Struct(Box::new(A::MoveStructLayout {
            type_: struct_tag(tag),
            fields: vec![A::MoveFieldLayout::new(ident("a"), A::MoveTypeLayout::U8)],
        })))
    };
    assert!(!mk("0x0::foo::A").equivalent(&mk("0x0::foo::B")));
}

#[test]
fn annotated_different_field_name_not_equivalent() {
    let mk = |name: &str| {
        compress_annotated(&A::MoveTypeLayout::Struct(Box::new(A::MoveStructLayout {
            type_: struct_tag("0x0::foo::Bar"),
            fields: vec![A::MoveFieldLayout::new(ident(name), A::MoveTypeLayout::U8)],
        })))
    };
    assert!(!mk("a").equivalent(&mk("b")));
}

#[test]
fn annotated_different_variant_tag_not_equivalent() {
    let mk = |tag: u16| {
        let mut variants = BTreeMap::new();
        variants.insert(
            (ident("V"), tag),
            vec![A::MoveFieldLayout::new(ident("x"), A::MoveTypeLayout::U8)],
        );
        compress_annotated(&A::MoveTypeLayout::Enum(Box::new(A::MoveEnumLayout {
            type_: struct_tag("0x0::foo::E"),
            variants,
        })))
    };
    assert!(!mk(0).equivalent(&mk(1)));
}

#[test]
fn annotated_different_variant_name_not_equivalent() {
    let mk = |name: &str| {
        let mut variants = BTreeMap::new();
        variants.insert(
            (ident(name), 0),
            vec![A::MoveFieldLayout::new(ident("x"), A::MoveTypeLayout::U8)],
        );
        compress_annotated(&A::MoveTypeLayout::Enum(Box::new(A::MoveEnumLayout {
            type_: struct_tag("0x0::foo::E"),
            variants,
        })))
    };
    assert!(!mk("V1").equivalent(&mk("V2")));
}

#[test]
fn annotated_unknown_variant_equivalence() {
    let tag = struct_tag("0x0::foo::E");
    let n_v = ident("V");
    let n_w = ident("W");
    let n_x = ident("x");

    let mut b1 = CA::MoveTypeLayoutBuilder::new();
    let u8_h1 = b1.u8();
    let v1_w_fields = [(&n_x, u8_h1)];
    let h1 = b1
        .enum_layout(
            &tag,
            &[(&n_v, 0u16, None), (&n_w, 1u16, Some(&v1_w_fields))],
        )
        .unwrap();
    let l1 = b1.build(h1);

    let mut b2 = CA::MoveTypeLayoutBuilder::new();
    let u8_h2 = b2.u8();
    let v2_w_fields = [(&n_x, u8_h2)];
    let h2 = b2
        .enum_layout(
            &tag,
            &[(&n_v, 0u16, None), (&n_w, 1u16, Some(&v2_w_fields))],
        )
        .unwrap();
    let l2 = b2.build(h2);
    assert!(l1.equivalent(&l2));

    // Known with empty fields vs Unknown — not equivalent.
    let mut b3 = CA::MoveTypeLayoutBuilder::new();
    let u8_h3 = b3.u8();
    let v3_w_fields = [(&n_x, u8_h3)];
    let h3 = b3
        .enum_layout(
            &tag,
            &[(&n_v, 0u16, Some(&[])), (&n_w, 1u16, Some(&v3_w_fields))],
        )
        .unwrap();
    let l3 = b3.build(h3);
    assert!(!l1.equivalent(&l3));
}

#[test]
fn annotated_subtype_struct_equivalent() {
    let t = A::MoveTypeLayout::Struct(Box::new(A::MoveStructLayout {
        type_: struct_tag("0x0::foo::Bar"),
        fields: vec![A::MoveFieldLayout::new(ident("a"), A::MoveTypeLayout::U8)],
    }));
    let a = compress_annotated(&t);
    let b = compress_with_shuffle_annotated(&t, 3);

    let CA::MoveLayoutView::Struct(sa) = a.as_view() else {
        panic!()
    };
    let CA::MoveLayoutView::Struct(sb) = b.as_view() else {
        panic!()
    };
    assert!(sa.equivalent(&sb));
}

// =============================================================================
// Mutation helpers (for proptests).
// =============================================================================

/// Mutate a runtime tree at one node so the result is structurally different.
/// Returns `None` if no mutation could be applied (only happens for trivial
/// degenerate inputs we don't generate).
fn mutate_runtime(t: &R::MoveTypeLayout, seed: u64) -> Option<R::MoveTypeLayout> {
    let mut rng = StdRng::seed_from_u64(seed);
    Some(mutate_runtime_inner(t, &mut rng))
}

fn mutate_runtime_inner(t: &R::MoveTypeLayout, rng: &mut StdRng) -> R::MoveTypeLayout {
    use R::MoveTypeLayout as T;
    // Always alter the leaf type at the deepest reachable level — guarantees
    // the resulting tree differs in at least one observable spot.
    match t {
        T::Bool => T::U8,
        T::U8 => T::U16,
        T::U16 => T::U32,
        T::U32 => T::U64,
        T::U64 => T::U128,
        T::U128 => T::U256,
        T::U256 => T::Address,
        T::Address => T::Signer,
        T::Signer => T::Bool,
        T::Vector(inner) => T::Vector(Box::new(mutate_runtime_inner(inner, rng))),
        T::Struct(s) => {
            if s.0.is_empty() {
                // Add a field so the arity differs.
                T::Struct(Box::new(R::MoveStructLayout(Box::new(vec![T::Bool]))))
            } else {
                let i = rng.gen_range(0..s.0.len());
                let mut new_fields = (*s.0).clone();
                new_fields[i] = mutate_runtime_inner(&new_fields[i], rng);
                T::Struct(Box::new(R::MoveStructLayout(Box::new(new_fields))))
            }
        }
        T::Enum(e) => {
            if e.0.is_empty() {
                T::Enum(Box::new(R::MoveEnumLayout(Box::new(vec![vec![T::Bool]]))))
            } else {
                let vi = rng.gen_range(0..e.0.len());
                let mut new_variants = (*e.0).clone();
                if new_variants[vi].is_empty() {
                    new_variants[vi].push(T::Bool);
                } else {
                    let fi = rng.gen_range(0..new_variants[vi].len());
                    new_variants[vi][fi] = mutate_runtime_inner(&new_variants[vi][fi], rng);
                }
                T::Enum(Box::new(R::MoveEnumLayout(Box::new(new_variants))))
            }
        }
    }
}

/// Mutate an annotated tree so the result is structurally different.
fn mutate_annotated(t: &A::MoveTypeLayout, seed: u64) -> Option<A::MoveTypeLayout> {
    let mut rng = StdRng::seed_from_u64(seed);
    Some(mutate_annotated_inner(t, &mut rng))
}

fn mutate_annotated_inner(t: &A::MoveTypeLayout, rng: &mut StdRng) -> A::MoveTypeLayout {
    use A::MoveTypeLayout as T;
    match t {
        T::Bool => T::U8,
        T::U8 => T::U16,
        T::U16 => T::U32,
        T::U32 => T::U64,
        T::U64 => T::U128,
        T::U128 => T::U256,
        T::U256 => T::Address,
        T::Address => T::Signer,
        T::Signer => T::Bool,
        T::Vector(inner) => T::Vector(Box::new(mutate_annotated_inner(inner, rng))),
        T::Struct(s) => {
            let mut new_fields = s.fields.clone();
            if new_fields.is_empty() {
                new_fields.push(A::MoveFieldLayout::new(ident("__added"), T::Bool));
            } else {
                let i = rng.gen_range(0..new_fields.len());
                new_fields[i].layout = mutate_annotated_inner(&new_fields[i].layout, rng);
            }
            T::Struct(Box::new(A::MoveStructLayout {
                type_: s.type_.clone(),
                fields: new_fields,
            }))
        }
        T::Enum(e) => {
            if e.variants.is_empty() {
                let mut variants = BTreeMap::new();
                variants.insert((ident("__added"), 0), vec![]);
                T::Enum(Box::new(A::MoveEnumLayout {
                    type_: e.type_.clone(),
                    variants,
                }))
            } else {
                let mut entries: Vec<((Identifier, u16), Vec<A::MoveFieldLayout>)> = e
                    .variants
                    .iter()
                    .map(|(k, v)| (k.clone(), v.clone()))
                    .collect();
                let vi = rng.gen_range(0..entries.len());
                if entries[vi].1.is_empty() {
                    entries[vi]
                        .1
                        .push(A::MoveFieldLayout::new(ident("__added"), T::Bool));
                } else {
                    let fi = rng.gen_range(0..entries[vi].1.len());
                    entries[vi].1[fi].layout =
                        mutate_annotated_inner(&entries[vi].1[fi].layout, rng);
                }
                let variants: BTreeMap<(Identifier, u16), Vec<A::MoveFieldLayout>> =
                    entries.into_iter().collect();
                T::Enum(Box::new(A::MoveEnumLayout {
                    type_: e.type_.clone(),
                    variants,
                }))
            }
        }
    }
}

// =============================================================================
// Proptests
// =============================================================================

use crate::proptest_types::{arb_annotated_layout, arb_runtime_layout};
use proptest::prelude::*;

proptest! {
    #![proptest_config(ProptestConfig { cases: 256, ..ProptestConfig::default() })]

    // --- Runtime ---

    #[test]
    fn pt_runtime_reflexive(t in arb_runtime_layout()) {
        let c = compress_runtime(&t);
        prop_assert!(c.equivalent(&c));
    }

    #[test]
    fn pt_runtime_clone(t in arb_runtime_layout()) {
        let c = compress_runtime(&t);
        prop_assert!(c.equivalent(&c.clone()));
    }

    #[test]
    fn pt_runtime_pool_permutation_invariant(t in arb_runtime_layout(), seed in any::<u64>()) {
        let a = compress_runtime(&t);
        let b = compress_with_shuffle_runtime(&t, seed);
        prop_assert!(a.equivalent(&b));
    }

    #[test]
    fn pt_runtime_symmetric(t1 in arb_runtime_layout(), t2 in arb_runtime_layout()) {
        let a = compress_runtime(&t1);
        let b = compress_runtime(&t2);
        prop_assert_eq!(a.equivalent(&b), b.equivalent(&a));
    }

    #[test]
    fn pt_runtime_transitive(t in arb_runtime_layout(), s1 in any::<u64>(), s2 in any::<u64>()) {
        let a = compress_runtime(&t);
        let b = compress_with_shuffle_runtime(&t, s1);
        let c = compress_with_shuffle_runtime(&t, s2);
        // a~b and b~c by pool-permutation invariance, so a~c must hold.
        prop_assert!(a.equivalent(&b));
        prop_assert!(b.equivalent(&c));
        prop_assert!(a.equivalent(&c));
    }

    #[test]
    fn pt_runtime_mutation_not_equivalent(t in arb_runtime_layout(), seed in any::<u64>()) {
        let mutated = mutate_runtime(&t, seed).unwrap();
        let a = compress_runtime(&t);
        let b = compress_runtime(&mutated);
        prop_assert!(!a.equivalent(&b));
    }

    #[test]
    fn pt_runtime_view_consistency(t in arb_runtime_layout(), seed in any::<u64>()) {
        let a = compress_runtime(&t);
        let b = compress_with_shuffle_runtime(&t, seed);
        prop_assert!(a.as_view().equivalent(&b.as_view()));
    }

    // --- Annotated ---

    #[test]
    fn pt_annotated_reflexive(t in arb_annotated_layout()) {
        let c = compress_annotated(&t);
        prop_assert!(c.equivalent(&c));
    }

    #[test]
    fn pt_annotated_clone(t in arb_annotated_layout()) {
        let c = compress_annotated(&t);
        prop_assert!(c.equivalent(&c.clone()));
    }

    #[test]
    fn pt_annotated_pool_permutation_invariant(
        t in arb_annotated_layout(),
        seed in any::<u64>(),
    ) {
        let a = compress_annotated(&t);
        let b = compress_with_shuffle_annotated(&t, seed);
        prop_assert!(a.equivalent(&b));
    }

    #[test]
    fn pt_annotated_symmetric(
        t1 in arb_annotated_layout(),
        t2 in arb_annotated_layout(),
    ) {
        let a = compress_annotated(&t1);
        let b = compress_annotated(&t2);
        prop_assert_eq!(a.equivalent(&b), b.equivalent(&a));
    }

    #[test]
    fn pt_annotated_transitive(
        t in arb_annotated_layout(),
        s1 in any::<u64>(),
        s2 in any::<u64>(),
    ) {
        let a = compress_annotated(&t);
        let b = compress_with_shuffle_annotated(&t, s1);
        let c = compress_with_shuffle_annotated(&t, s2);
        prop_assert!(a.equivalent(&b));
        prop_assert!(b.equivalent(&c));
        prop_assert!(a.equivalent(&c));
    }

    #[test]
    fn pt_annotated_mutation_not_equivalent(
        t in arb_annotated_layout(),
        seed in any::<u64>(),
    ) {
        let mutated = mutate_annotated(&t, seed).unwrap();
        let a = compress_annotated(&t);
        let b = compress_annotated(&mutated);
        prop_assert!(!a.equivalent(&b));
    }

    #[test]
    fn pt_annotated_view_consistency(
        t in arb_annotated_layout(),
        seed in any::<u64>(),
    ) {
        let a = compress_annotated(&t);
        let b = compress_with_shuffle_annotated(&t, seed);
        prop_assert!(a.as_view().equivalent(&b.as_view()));
    }
}
