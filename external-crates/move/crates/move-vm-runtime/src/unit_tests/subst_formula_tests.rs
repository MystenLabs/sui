// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

//! Tests for the type size formulas: the syntactic pair (`type_size`, `type_depth`, closed
//! [`LinearFormula`]/[`MaxPlusFormula`]s per term) and the through-field pair (`value_depth`,
//! `layout_size`, partial forms on datatype descriptors and signature terms).
//!
//! The load-bearing property is *legacy equivalence*: the formula-predicted sizes of a
//! substitution must match the node/depth counters of the historical checked traversal exactly,
//! so that checked `subst` accepts and rejects precisely the same instantiations it always did.
//! We keep a small reference implementation of the legacy counters here and compare against it
//! over a family of generated type shapes.
//!
//! Substitution is defined on static (arena) terms only, so test base terms are built as
//! `ArenaType`s (mirrored from runtime terms via [`to_arena`], which keeps the shapes readable);
//! arguments are runtime `Type`s, as in the real substitution sites.

use crate::{
    cache::{arena::ArenaBuilder, identifier_interner::IdentifierInterner},
    execution::dispatch_tables::VirtualTableKey,
    jit::execution::ast::{
        ArenaType, DatatypeSizeInfo, DatatypeSizes, LinearFormula, MaxPlusFormula,
        PartialTypeFormula, Type, TypeArguments, TypeSize, check_syntactic_limits,
    },
    shared::constants::{MAX_TYPE_INSTANTIATION_NODES, TYPE_DEPTH_MAX},
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

fn dt(children: Vec<Type>) -> Type {
    Type::DatatypeInstantiation(Box::new((dt_key(), children)))
}

/// Mirror a runtime type term into `arena` as an `ArenaType`, so tests can write terms in the
/// readable runtime syntax and still exercise the arena-term substitution entry points.
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

/// Build `TypeArguments` for tests. The through-field quantities normally come from the
/// dispatch tables; term-level substitution only reads the syntactic pair, so nominal values
/// suffice here.
fn ty_args(types: Vec<Type>) -> TypeArguments {
    TypeArguments::new(types, |ty| {
        let (type_size, type_depth) = ty.syntactic_sizes();
        Ok(TypeSize {
            type_size,
            type_depth,
            value_depth: 1,
            layout_size: 1,
        })
    })
    .unwrap()
}

fn nested_vec(nodes: u64) -> Type {
    let mut t = Type::U128;
    for _ in 1..nodes {
        t = Type::Vector(Box::new(t));
    }
    t
}

/// The syntactic `(type_size, type_depth)` pairs of `types`, as solve arguments.
fn arg_sizes(types: &[Type]) -> Vec<(u64, u64)> {
    types.iter().map(|ty| ty.syntactic_sizes()).collect()
}

// -------------------------------------------------------------------------------------------------
// Reference implementation of the legacy traversal counters
// -------------------------------------------------------------------------------------------------

#[derive(Default)]
struct LegacyCounters {
    nodes: u64,
    max_depth: u64,
}

/// The legacy traversal counted every node it entered, one level deeper than its parent. Cloning
/// an argument in for a `TyParam` occurrence entered the argument's nodes *below* the occurrence
/// (the parameter node itself was already counted).
fn legacy_clone(ty: &Type, depth: u64, c: &mut LegacyCounters) {
    let depth = depth + 1;
    c.nodes += 1;
    c.max_depth = c.max_depth.max(depth);
    match ty {
        Type::Vector(t) | Type::Reference(t) | Type::MutableReference(t) => {
            legacy_clone(t, depth, c)
        }
        Type::DatatypeInstantiation(inst) => {
            for t in &inst.1 {
                legacy_clone(t, depth, c);
            }
        }
        _ => (),
    }
}

fn legacy_subst(ty: &Type, ty_args: &[Type], depth: u64, c: &mut LegacyCounters) {
    let depth = depth + 1;
    c.nodes += 1;
    c.max_depth = c.max_depth.max(depth);
    match ty {
        Type::TyParam(idx) => legacy_clone(&ty_args[*idx as usize], depth, c),
        Type::Vector(t) | Type::Reference(t) | Type::MutableReference(t) => {
            legacy_subst(t, ty_args, depth, c)
        }
        Type::DatatypeInstantiation(inst) => {
            for t in &inst.1 {
                legacy_subst(t, ty_args, depth, c);
            }
        }
        _ => (),
    }
}

// -------------------------------------------------------------------------------------------------
// Deterministic type generator
// -------------------------------------------------------------------------------------------------

fn next(seed: &mut u64) -> u64 {
    *seed = seed
        .wrapping_mul(6364136223846793005)
        .wrapping_add(1442695040888963407);
    *seed >> 33
}

/// Generate a pseudo-random type term with up to `n_params` distinct type parameters and nesting
/// bounded by `budget`. Shapes need not be valid Move types (e.g. nested references); the
/// size algebra is uniform over the term structure.
fn gen_type(seed: &mut u64, budget: u64, n_params: u16) -> Type {
    let choice = if budget == 0 {
        next(seed) % 8
    } else {
        next(seed) % 12
    };
    match choice {
        0 => Type::Bool,
        1 => Type::U8,
        2 => Type::U16,
        3 => Type::U64,
        4 => Type::Address,
        5 => Type::Signer,
        6 => Type::Datatype(dt_key()),
        7 => {
            if n_params > 0 {
                Type::TyParam((next(seed) % n_params as u64) as u16)
            } else {
                Type::U128
            }
        }
        8 => Type::Vector(Box::new(gen_type(seed, budget - 1, n_params))),
        9 => Type::Reference(Box::new(gen_type(seed, budget - 1, n_params))),
        10 => Type::MutableReference(Box::new(gen_type(seed, budget - 1, n_params))),
        _ => {
            let children = (0..(next(seed) % 3 + 1))
                .map(|_| gen_type(seed, budget - 1, n_params))
                .collect();
            dt(children)
        }
    }
}

// -------------------------------------------------------------------------------------------------
// Tests
// -------------------------------------------------------------------------------------------------

/// A worked example with hand-computed numbers, for readability:
/// `T = S<Vector<T0>, T1, T0>` applied to `[Vector<Vector<U8>>, U8]`.
#[test]
fn formula_hand_example() {
    let arena = ArenaBuilder::new_bounded();
    let base_term = dt(vec![
        Type::Vector(Box::new(Type::TyParam(0))),
        Type::TyParam(1),
        Type::TyParam(0),
    ]);
    let base = to_arena(&arena, &base_term);
    // Base term: 5 nodes (S, Vector, T0, T1, T0); T0 occurs twice, deepest at depth 3;
    // T1 occurs once at depth 2.
    let arg0 = nested_vec(3); // Vector<Vector<U8>>: 3 nodes, depth 3
    let arg1 = Type::U8; // 1 node, depth 1
    assert_eq!(arg0.syntactic_sizes(), (3, 3));
    assert_eq!(arg1.syntactic_sizes(), (1, 1));

    let (size_formula, depth_formula) = base.syntactic_formulas();
    let args = [arg0.clone(), arg1.clone()];
    let sizes = arg_sizes(&args);
    // nodes = 5 + 2×3 + 1×1 = 12; depth = max(3, 3+3, 2+1) = 6.
    assert_eq!(
        size_formula.solve_with(&sizes, |(size, _)| *size).unwrap(),
        12
    );
    assert_eq!(
        depth_formula
            .solve_with(&sizes, |(_, depth)| *depth)
            .unwrap(),
        6
    );

    // The predicted sizes are well within the limits, so the substitution goes through and
    // builds exactly the expected type.
    let result = base.subst(&args).unwrap();
    assert_eq!(
        result,
        dt(vec![
            Type::Vector(Box::new(arg0.clone())),
            Type::U8,
            arg0.clone(),
        ])
    );
}

/// `check_syntactic_limits` accepts exactly at the limits and rejects just above them, with the
/// depth limit checked first.
#[test]
fn syntactic_limit_boundaries() {
    assert!(check_syntactic_limits(MAX_TYPE_INSTANTIATION_NODES, TYPE_DEPTH_MAX).is_ok());

    let err = check_syntactic_limits(MAX_TYPE_INSTANTIATION_NODES + 1, TYPE_DEPTH_MAX).unwrap_err();
    assert_eq!(err.major_status(), StatusCode::VM_MAX_TYPE_NODES_REACHED);

    let err = check_syntactic_limits(MAX_TYPE_INSTANTIATION_NODES, TYPE_DEPTH_MAX + 1).unwrap_err();
    assert_eq!(err.major_status(), StatusCode::VM_MAX_TYPE_DEPTH_REACHED);

    // Depth is checked first when both limits are exceeded.
    let err =
        check_syntactic_limits(MAX_TYPE_INSTANTIATION_NODES + 1, TYPE_DEPTH_MAX + 1).unwrap_err();
    assert_eq!(err.major_status(), StatusCode::VM_MAX_TYPE_DEPTH_REACHED);
}

/// The formulas' predictions must equal the legacy traversal's counters on every shape, and
/// checked `subst` must accept/reject exactly on that boundary.
#[test]
fn formulas_match_legacy_traversal_counters() {
    let mut seed = 0x5EED_CAFE_u64;
    for case in 0..500u64 {
        let arena = ArenaBuilder::new_bounded();
        let n_params = (case % 4) as u16 + 1;
        let base_term = gen_type(&mut seed, 4, n_params);
        let base = to_arena(&arena, &base_term);
        let args = (0..n_params)
            .map(|_| gen_type(&mut seed, 3, 0))
            .collect::<Vec<_>>();

        let mut legacy = LegacyCounters::default();
        legacy_subst(&base_term, &args, 0, &mut legacy);

        let (size_formula, depth_formula) = base.syntactic_formulas();
        let sizes = arg_sizes(&args);
        let predicted_size = size_formula.solve_with(&sizes, |(size, _)| *size).unwrap();
        let predicted_depth = depth_formula
            .solve_with(&sizes, |(_, depth)| *depth)
            .unwrap();
        assert_eq!(predicted_size, legacy.nodes, "nodes mismatch, case {case}");
        assert_eq!(
            predicted_depth, legacy.max_depth,
            "depth mismatch, case {case}"
        );

        // Checked `subst` accepts or rejects exactly as the predicted sizes dictate against
        // the (constant) limits.
        let result = base.subst(&args);
        if predicted_depth > TYPE_DEPTH_MAX {
            assert_eq!(
                result.unwrap_err().major_status(),
                StatusCode::VM_MAX_TYPE_DEPTH_REACHED,
                "case {case}"
            );
        } else if predicted_size > MAX_TYPE_INSTANTIATION_NODES {
            assert_eq!(
                result.unwrap_err().major_status(),
                StatusCode::VM_MAX_TYPE_NODES_REACHED,
                "case {case}"
            );
        } else {
            assert!(result.is_ok(), "subst failed under the limits, case {case}");
        }
    }
}

/// Checked `subst` rejects exactly on the constant limits: a vector chain sized right at the
/// node limit passes, one past it fails, and a chain deeper than the depth limit fails with the
/// depth error (depth is checked first).
#[test]
fn subst_rejects_on_constant_limits() {
    let base = ArenaType::TyParam(0);
    // The parameter occurrence itself is counted in addition to the argument's nodes (legacy
    // semantics), so the predicted sizes are the argument's plus one node and one level.
    assert!(
        base.subst(&[nested_vec(MAX_TYPE_INSTANTIATION_NODES - 1)])
            .is_ok()
    );
    let err = base
        .subst(&[nested_vec(MAX_TYPE_INSTANTIATION_NODES)])
        .unwrap_err();
    assert_eq!(err.major_status(), StatusCode::VM_MAX_TYPE_NODES_REACHED);
    let err = base.subst(&[nested_vec(TYPE_DEPTH_MAX)]).unwrap_err();
    assert_eq!(err.major_status(), StatusCode::VM_MAX_TYPE_DEPTH_REACHED);
}

/// The translation-time formulas ([`PartialTypeFormula::for_term`], as stored in loaded
/// packages) must agree with the on-the-fly route ([`ArenaType::syntactic_formulas`] /
/// [`ArenaType::subst`]) on predictions, the built type, and the accept/reject boundary.
#[test]
fn precomputed_formulas_match_on_the_fly_subst() {
    let arena = ArenaBuilder::new_bounded();
    // Vector<Vector<T0>>, twice: one term consumed by `for_term`, one kept for the on-the-fly
    // route.
    let term = Type::Vector(Box::new(Type::Vector(Box::new(Type::TyParam(0)))));
    let precomputed = PartialTypeFormula::for_term(to_arena(&arena, &term), &arena).unwrap();
    let on_the_fly = to_arena(&arena, &term);

    let args = ty_args(vec![nested_vec(4)]);
    let (size_formula, depth_formula) = on_the_fly.syntactic_formulas();

    // Same predictions...
    assert_eq!(
        precomputed
            .type_size
            .solve_with(args.sizes(), |sizes| sizes.type_size)
            .unwrap(),
        size_formula
            .solve_with(args.sizes(), |sizes| sizes.type_size)
            .unwrap()
    );
    assert_eq!(
        precomputed
            .type_depth
            .solve_with(args.sizes(), |sizes| sizes.type_depth)
            .unwrap(),
        depth_formula
            .solve_with(args.sizes(), |sizes| sizes.type_depth)
            .unwrap()
    );
    // ...same built type...
    assert_eq!(
        precomputed.term.subst(args.types()).unwrap(),
        on_the_fly.subst(args.types()).unwrap()
    );

    // ...and the same accept/reject boundary at the constant limits. The term contributes
    // three nodes (two vectors plus the parameter occurrence) on top of the argument.
    let at_limit = nested_vec(MAX_TYPE_INSTANTIATION_NODES - 3);
    assert!(
        precomputed
            .term
            .subst(std::slice::from_ref(&at_limit))
            .is_ok()
    );
    assert!(on_the_fly.subst(&[at_limit]).is_ok());
    let over_limit = nested_vec(MAX_TYPE_INSTANTIATION_NODES - 2);
    let err = precomputed
        .term
        .subst(std::slice::from_ref(&over_limit))
        .unwrap_err();
    assert_eq!(err.major_status(), StatusCode::VM_MAX_TYPE_NODES_REACHED);
    let err = on_the_fly.subst(&[over_limit]).unwrap_err();
    assert_eq!(err.major_status(), StatusCode::VM_MAX_TYPE_NODES_REACHED);
}

/// Substituting closed formulas into a closed formula must equal solving the composition: for
/// all `f`, `gs`, `xs`: `f.subst(gs).solve(xs) == f.solve(gs.map(|g| g.solve(xs)))`. This is
/// the law the link-time closing step rests on (closure under substitution), for both
/// algebras.
#[test]
fn closed_formula_subst_composes() {
    let mut seed = 0xC0_FFEE_u64;
    let gen_terms = |seed: &mut u64, n_params: u64| -> Vec<(u16, u64)> {
        let mut terms = vec![];
        for param in 0..n_params {
            if next(seed).is_multiple_of(2) {
                terms.push((param as u16, next(seed) % 5));
            }
        }
        terms
    };
    for case in 0..500u64 {
        // f is over 3 parameters; each g and the solve point are over 2.
        let f_linear = LinearFormula {
            constant: next(&mut seed) % 10,
            terms: gen_terms(&mut seed, 3),
        };
        let f_maxplus = MaxPlusFormula {
            constant: next(&mut seed) % 10,
            terms: gen_terms(&mut seed, 3),
        };
        let gs_linear = (0..3)
            .map(|_| LinearFormula {
                constant: next(&mut seed) % 10,
                terms: gen_terms(&mut seed, 2),
            })
            .collect::<Vec<_>>();
        let gs_maxplus = (0..3)
            .map(|_| MaxPlusFormula {
                constant: next(&mut seed) % 10,
                terms: gen_terms(&mut seed, 2),
            })
            .collect::<Vec<_>>();
        let xs = [next(&mut seed) % 10, next(&mut seed) % 10];

        let composed = f_linear.subst(&gs_linear).unwrap().solve(&xs).unwrap();
        let direct = f_linear
            .solve(
                &gs_linear
                    .iter()
                    .map(|g| g.solve(&xs).unwrap())
                    .collect::<Vec<_>>(),
            )
            .unwrap();
        assert_eq!(composed, direct, "linear composition mismatch, case {case}");

        let composed = f_maxplus.subst(&gs_maxplus).unwrap().solve(&xs).unwrap();
        let direct = f_maxplus
            .solve(
                &gs_maxplus
                    .iter()
                    .map(|g| g.solve(&xs).unwrap())
                    .collect::<Vec<_>>(),
            )
            .unwrap();
        assert_eq!(
            composed, direct,
            "max-plus composition mismatch, case {case}"
        );
    }
}

/// The datatype-level through-field forms computed at translation time: purely local field
/// structure folds into the constants, type parameters stay parameter terms, and datatype
/// applications stay pending [`ApplyFormula`]s for the dispatch tables to close under a
/// transaction's linkage view — and a fully concrete datatype is just written down as
/// constants.
#[test]
fn datatype_size_info() {
    let arena = ArenaBuilder::new_bounded();

    // struct C { a: u64, b: vector<u32> } — fully concrete: constants, no formula.
    // value_depth = max(S→u64, S→vector→u32) = 3; layout_size = C + u64 + vector + u32 = 4.
    let fields = [
        ArenaType::U64,
        ArenaType::Vector(arena.alloc_box(ArenaType::U32).unwrap()),
    ];
    let info = DatatypeSizeInfo::for_datatype_fields(fields.iter(), 0, &arena).unwrap();
    assert!(matches!(
        info,
        DatatypeSizeInfo::Constant(DatatypeSizes {
            value_depth: 3,
            layout_size: 4,
        })
    ));

    // struct S<T0> { a: u64, b: vector<T0> } — formulas over T0.
    let fields = [
        ArenaType::U64,
        ArenaType::Vector(arena.alloc_box(ArenaType::TyParam(0)).unwrap()),
    ];
    let info = DatatypeSizeInfo::for_datatype_fields(fields.iter(), 0, &arena).unwrap();
    let DatatypeSizeInfo::Formula {
        value_depth,
        layout_size,
    } = info
    else {
        panic!("expected formulas for a generic datatype");
    };
    // value_depth(S<arg>) = max(2, 2 + value_depth(arg)); layout_size(S<arg>) = 3 +
    // layout(arg): S + u64 + vector, plus the argument's layout once.
    assert_eq!(value_depth.constant, 2);
    assert_eq!(value_depth.params.as_ref(), &[(0, 2)]);
    assert!(value_depth.applies.is_empty());
    assert_eq!(layout_size.constant, 3);
    assert_eq!(layout_size.params.as_ref(), &[(0, 1)]);
    assert!(layout_size.applies.is_empty());

    // struct P { s: SomeOtherStruct } — the field's datatype application is a pending
    // `ApplyFormula` one nesting level below `P` itself, occurring once.
    let fields = [ArenaType::Datatype(dt_key())];
    let info = DatatypeSizeInfo::for_datatype_fields(fields.iter(), 0, &arena).unwrap();
    let DatatypeSizeInfo::Formula {
        value_depth,
        layout_size,
    } = info
    else {
        panic!("expected formulas for a datatype with datatype fields");
    };
    assert_eq!(value_depth.constant, 1);
    assert!(value_depth.params.is_empty());
    assert_eq!(value_depth.applies.len(), 1);
    let (offset, apply) = &value_depth.applies[0];
    assert_eq!(*offset, 1);
    assert!(apply.args.is_empty());
    assert_eq!(layout_size.constant, 1);
    assert!(layout_size.params.is_empty());
    assert_eq!(layout_size.applies.len(), 1);
    let (multiplicity, apply) = &layout_size.applies[0];
    assert_eq!(*multiplicity, 1);
    assert!(apply.args.is_empty());

    // enum E<T0> { A { a: T0 }, B { b: u8 } } — one layout node per variant on top of the
    // datatype's own node.
    let fields = [ArenaType::TyParam(0), ArenaType::U8];
    let info =
        DatatypeSizeInfo::for_datatype_fields(fields.iter(), /* variants */ 2, &arena).unwrap();
    let DatatypeSizeInfo::Formula {
        value_depth,
        layout_size,
    } = info
    else {
        panic!("expected formulas for a generic enum");
    };
    assert_eq!(value_depth.constant, 2);
    assert_eq!(value_depth.params.as_ref(), &[(0, 1)]);
    assert_eq!(layout_size.constant, 4); // E + 2 variants + u8
    assert_eq!(layout_size.params.as_ref(), &[(0, 1)]);
}

/// `syntactic_sizes` must agree with a naive node count (the semantics of the old
/// `count_type_nodes`, which it subsumes), on both type representations.
#[test]
fn syntactic_sizes_match_naive_node_count() {
    fn naive_count(ty: &Type) -> u64 {
        match ty {
            Type::Vector(t) | Type::Reference(t) | Type::MutableReference(t) => 1 + naive_count(t),
            Type::DatatypeInstantiation(inst) => 1 + inst.1.iter().map(naive_count).sum::<u64>(),
            _ => 1,
        }
    }
    let mut seed = 0xBADC_0FFE_u64;
    for _ in 0..200 {
        let arena = ArenaBuilder::new_bounded();
        let ty = gen_type(&mut seed, 4, 3);
        assert_eq!(ty.syntactic_sizes().0, naive_count(&ty));
        assert_eq!(
            to_arena(&arena, &ty).syntactic_sizes(),
            ty.syntactic_sizes()
        );
    }
}

/// The DoS shape from the type-instantiation fix: a term mentioning one parameter many times,
/// applied to a large argument. The formula rejects it by arithmetic before any part of the
/// oversized type is built.
#[test]
fn quadratic_instantiation_rejected_before_construction() {
    let arena = ArenaBuilder::new_bounded();
    let base = to_arena(&arena, &dt(vec![Type::TyParam(0); 32]));
    let arg = nested_vec(95);

    // Predicted: 33 base nodes + 32 occurrences × 95 nodes.
    let (size_formula, _) = base.syntactic_formulas();
    let predicted = size_formula
        .solve_with(&arg_sizes(std::slice::from_ref(&arg)), |(size, _)| *size)
        .unwrap();
    assert_eq!(predicted, 33 + 32 * 95);

    let err = base.subst(&[arg]).unwrap_err();
    assert_eq!(err.major_status(), StatusCode::VM_MAX_TYPE_NODES_REACHED);
}

#[test]
fn missing_type_argument_is_an_invariant_violation() {
    let arena = ArenaBuilder::new_bounded();
    let base = ArenaType::Vector(arena.alloc_box(ArenaType::TyParam(1)).unwrap());
    let (size_formula, _) = base.syntactic_formulas();
    let err = size_formula.solve(&[1]).unwrap_err();
    assert_eq!(
        err.major_status(),
        StatusCode::UNKNOWN_INVARIANT_VIOLATION_ERROR
    );
    let err = base.subst(&[Type::U8]).unwrap_err();
    assert_eq!(
        err.major_status(),
        StatusCode::UNKNOWN_INVARIANT_VIOLATION_ERROR
    );
}
