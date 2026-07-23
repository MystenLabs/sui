// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

//! Tests for the type size formula algebra (`LinearForm`/`MaxPlusForm`, the partial and arena
//! bundles). The load-bearing property is *closure under substitution*: substituting argument
//! forms into a formula and then solving equals solving each argument form and then the formula.
//! That is what lets the runtime resolve a datatype's formula once and solve it against concrete
//! argument sizes later and still get the size it would have measured on the fully realized type.
//! We check it over a family of generated forms, and spot-check the JIT datatype builder against a
//! hand computation.
//!
//! The end-to-end resolution path (against a real linkage) is exercised by the loader and
//! instantiation tests, which run the whole VM; here we test the pure algebra in isolation.

use crate::{
    cache::{arena::ArenaBuilder, identifier_interner::IdentifierInterner},
    execution::dispatch_tables::VirtualTableKey,
    jit::execution::ast::{ArenaType, Type},
    shared::{
        constants::{MAX_TYPE_INSTANTIATION_NODES, TYPE_DEPTH_MAX},
        type_size_formulae::{
            ArenaTypeSizeFormula, LinearForm, LinearTerm, MaxPlusForm, MaxPlusTerm,
            PartialTypeSizeFormula, TypeSize, check_syntactic_limits,
        },
    },
};
use move_core_types::{
    account_address::AccountAddress, identifier::Identifier, vm_status::StatusCode,
};

// -------------------------------------------------------------------------------------------------
// Helpers
// -------------------------------------------------------------------------------------------------

// NB: keys are interner indices, so two fresh interners hand out equal keys for the first
// identifier interned. We never resolve these keys, so the dangling interner is fine here.
fn dt_key() -> VirtualTableKey {
    let interner = IdentifierInterner::new();
    let name = interner.intern_identifier(&Identifier::new("m").unwrap());
    VirtualTableKey::from_parts(AccountAddress::TWO, name, name)
}

#[allow(dead_code)]
fn dt(children: Vec<Type>) -> Type {
    Type::DatatypeInstantiation(Box::new((dt_key(), children)))
}

/// Mirror a runtime type term into `arena` as an `ArenaType`, so tests can write terms in the
/// readable runtime syntax and still exercise the arena-term entry points.
pub(crate) fn to_arena(arena: &ArenaBuilder, ty: &Type) -> ArenaType {
    match ty {
        Type::Bool => ArenaType::Bool,
        Type::U8 => ArenaType::U8,
        Type::U16 => ArenaType::U16,
        Type::U32 => ArenaType::U32,
        Type::U64 => ArenaType::U64,
        Type::U128 => ArenaType::U128,
        Type::U256 => ArenaType::U256,
        Type::Address => ArenaType::Address,
        Type::Signer => ArenaType::Signer,
        Type::TyParam(idx) => ArenaType::TyParam(*idx),
        Type::Vector(t) => ArenaType::Vector(arena.alloc_box(to_arena(arena, t)).unwrap()),
        Type::Reference(t) => ArenaType::Reference(arena.alloc_box(to_arena(arena, t)).unwrap()),
        Type::MutableReference(t) => {
            ArenaType::MutableReference(arena.alloc_box(to_arena(arena, t)).unwrap())
        }
        Type::Datatype(key) => ArenaType::Datatype(key.clone()),
        Type::DatatypeInstantiation(inst) => {
            let (key, tys) = &**inst;
            let children = tys.iter().map(|t| to_arena(arena, t)).collect::<Vec<_>>();
            ArenaType::DatatypeInstantiation(
                arena
                    .alloc_box((key.clone(), arena.alloc_vec(children.into_iter()).unwrap()))
                    .unwrap(),
            )
        }
    }
}

#[allow(dead_code)]
fn nested_vec(nodes: u64) -> Type {
    let mut t = Type::U128;
    for _ in 1..nodes {
        t = Type::Vector(Box::new(t));
    }
    t
}

// A tiny deterministic PRNG so the generated forms are reproducible without `rand`.
fn next(seed: &mut u64) -> u64 {
    *seed = seed
        .wrapping_mul(6364136223846793005)
        .wrapping_add(1442695040888963407);
    *seed >> 33
}

/// A random linear form over `n_params` parameters. Built with `absorb`, so terms merge by
/// parameter. Coefficients and constant are kept small so no measure saturates.
fn rand_linear(seed: &mut u64, n_params: u16) -> LinearForm {
    let mut form = LinearForm::constant(next(seed) % 5);
    for _ in 0..(next(seed) % 4) {
        let param = (next(seed) % n_params.max(1) as u64) as u16;
        form.absorb(next(seed) % 4, &LinearForm::parameter(param));
    }
    form
}

/// A random max-plus form over `n_params` parameters. Built with `absorb`, so terms merge by
/// parameter (taking the max offset).
fn rand_maxplus(seed: &mut u64, n_params: u16) -> MaxPlusForm {
    let mut form = MaxPlusForm::constant(next(seed) % 5);
    for _ in 0..(next(seed) % 4) {
        let param = (next(seed) % n_params.max(1) as u64) as u16;
        form.absorb(next(seed) % 4, &MaxPlusForm::parameter(param));
    }
    form
}

// -------------------------------------------------------------------------------------------------
// Closure under substitution: substitute-then-solve == solve-args-then-solve
// -------------------------------------------------------------------------------------------------

#[test]
fn linear_form_substitute_composes() {
    let mut seed = 0x1234_5678u64;
    for _ in 0..2000 {
        let (p, q) = (3u16, 3u16);
        let f = rand_linear(&mut seed, p);
        let gs: Vec<LinearForm> = (0..p).map(|_| rand_linear(&mut seed, q)).collect();
        let xs: Vec<u64> = (0..q).map(|_| next(&mut seed) % 5).collect();

        let composed = f.substitute(&gs).unwrap().solve(&xs).unwrap();
        let solved_args: Vec<u64> = gs.iter().map(|g| g.solve(&xs).unwrap()).collect();
        let direct = f.solve(&solved_args).unwrap();
        assert_eq!(composed, direct);
    }
}

#[test]
fn maxplus_form_substitute_composes() {
    let mut seed = 0x9e37_79b9u64;
    for _ in 0..2000 {
        let (p, q) = (3u16, 3u16);
        let f = rand_maxplus(&mut seed, p);
        let gs: Vec<MaxPlusForm> = (0..p).map(|_| rand_maxplus(&mut seed, q)).collect();
        let xs: Vec<u64> = (0..q).map(|_| next(&mut seed) % 5).collect();

        let composed = f.substitute(&gs).unwrap().solve(&xs).unwrap();
        let solved_args: Vec<u64> = gs.iter().map(|g| g.solve(&xs).unwrap()).collect();
        let direct = f.solve(&solved_args).unwrap();
        assert_eq!(composed, direct);
    }
}

#[test]
fn partial_formula_substitute_composes() {
    let mut seed = 0xdead_beefu64;
    for _ in 0..1000 {
        let (p, q) = (2u16, 2u16);
        let mk = |seed: &mut u64, n| PartialTypeSizeFormula {
            type_size: rand_linear(seed, n),
            type_depth: rand_maxplus(seed, n),
            value_depth: rand_maxplus(seed, n),
            layout_size: rand_linear(seed, n),
        };
        let f = mk(&mut seed, p);
        let gs: Vec<PartialTypeSizeFormula> = (0..p).map(|_| mk(&mut seed, q)).collect();
        let xs: Vec<TypeSize> = (0..q)
            .map(|_| TypeSize {
                type_size: next(&mut seed) % 5,
                type_depth: next(&mut seed) % 5,
                value_depth: next(&mut seed) % 5,
                layout_size: next(&mut seed) % 5,
            })
            .collect();

        let composed = f.substitute(&gs).unwrap().solve(&xs).unwrap();
        let solved_args: Vec<TypeSize> = gs.iter().map(|g| g.solve(&xs).unwrap()).collect();
        let direct = f.solve(&solved_args).unwrap();
        assert_eq!(composed, direct);
    }
}

// -------------------------------------------------------------------------------------------------
// Partial formula primitives
// -------------------------------------------------------------------------------------------------

#[test]
fn primitive_solves_to_primitive() {
    let solved = PartialTypeSizeFormula::primitive().solve(&[]).unwrap();
    assert_eq!(solved, TypeSize::PRIMITIVE);
}

#[test]
fn wrap_adds_one_level_to_every_measure() {
    let inner = TypeSize {
        type_size: 3,
        type_depth: 2,
        value_depth: 4,
        layout_size: 5,
    };
    // A parameter formula solved against `inner` is `inner`; wrapping it must add one everywhere.
    let wrapped = PartialTypeSizeFormula::parameter(0)
        .wrap()
        .solve(&[inner])
        .unwrap();
    assert_eq!(wrapped, TypeSize::wrap(inner));
}

// -------------------------------------------------------------------------------------------------
// The JIT datatype builder
// -------------------------------------------------------------------------------------------------

#[test]
fn for_datatype_struct_forms() {
    // struct S<T> { a: T, b: vector<T>, c: u64 }
    let arena = ArenaBuilder::new_bounded();
    let fields = [
        to_arena(&arena, &Type::TyParam(0)),
        to_arena(&arena, &Type::Vector(Box::new(Type::TyParam(0)))),
        to_arena(&arena, &Type::U64),
    ];
    let formula = ArenaTypeSizeFormula::for_datatype(1, fields.iter(), 0, &arena).unwrap();

    // type_size(S<T>) = 1 + type_size(T); type_depth(S<T>) = max(1, 1 + depth(T)).
    assert_eq!(
        formula.type_size,
        LinearForm {
            constant: 1,
            terms: vec![LinearTerm {
                param: 0,
                coefficient: 1
            }],
        }
    );
    assert_eq!(
        formula.type_depth,
        MaxPlusForm {
            constant: 1,
            terms: vec![MaxPlusTerm {
                param: 0,
                offset: 1
            }],
        }
    );

    // No datatype-application fields, so nothing stays symbolic.
    assert!(formula.apps.is_empty());

    // value_depth: the deepest field is `vector<T>` (T two levels below S), and the primitive
    // `c` bottoms out at level 2 → max(2, 2 + value_depth(T)).
    assert_eq!(
        formula.value_depth_local,
        MaxPlusForm {
            constant: 2,
            terms: vec![MaxPlusTerm {
                param: 0,
                offset: 2
            }],
        }
    );
    // layout_size: S node + T (from `a`) + vector node + T (from `b`) + u64 node = 3 + 2·T.
    assert_eq!(
        formula.layout_size_local,
        LinearForm {
            constant: 3,
            terms: vec![LinearTerm {
                param: 0,
                coefficient: 2
            }],
        }
    );

    // Cross-check by solving against T = u64 (every measure 1).
    let concrete = PartialTypeSizeFormula {
        type_size: formula.type_size.clone(),
        type_depth: formula.type_depth.clone(),
        value_depth: formula.value_depth_local.clone(),
        layout_size: formula.layout_size_local.clone(),
    }
    .solve(&[TypeSize::PRIMITIVE])
    .unwrap();
    assert_eq!(
        concrete,
        TypeSize {
            type_size: 2,   // S<u64>: S + u64
            type_depth: 2,  // S over u64
            value_depth: 3, // S value nests vector<u64> (depth 2)
            layout_size: 5, // 3 + 2·1
        }
    );
}

#[test]
fn for_datatype_enum_counts_a_node_per_variant() {
    // enum E<T> { A(T), B(T, u64) } — two variants, so one extra layout node beyond the fields.
    let arena = ArenaBuilder::new_bounded();
    let fields = [
        to_arena(&arena, &Type::TyParam(0)),
        to_arena(&arena, &Type::TyParam(0)),
        to_arena(&arena, &Type::U64),
    ];
    let formula = ArenaTypeSizeFormula::for_datatype(1, fields.iter(), 2, &arena).unwrap();
    // layout: E node + 2 variant nodes + 2·T (one per `T` field) + u64 node = 4 + 2·T.
    assert_eq!(
        formula.layout_size_local,
        LinearForm {
            constant: 4,
            terms: vec![LinearTerm {
                param: 0,
                coefficient: 2
            }],
        }
    );
}

// -------------------------------------------------------------------------------------------------
// Limit checks
// -------------------------------------------------------------------------------------------------

#[test]
fn check_syntactic_limits_boundaries() {
    assert!(check_syntactic_limits(MAX_TYPE_INSTANTIATION_NODES, TYPE_DEPTH_MAX).is_ok());

    let node_err = check_syntactic_limits(MAX_TYPE_INSTANTIATION_NODES + 1, 1).unwrap_err();
    assert_eq!(
        node_err.major_status(),
        StatusCode::VM_MAX_TYPE_NODES_REACHED
    );

    let depth_err = check_syntactic_limits(1, TYPE_DEPTH_MAX + 1).unwrap_err();
    assert_eq!(
        depth_err.major_status(),
        StatusCode::VM_MAX_TYPE_DEPTH_REACHED
    );
}
