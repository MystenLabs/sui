// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

//! Tests for `TypeMeasure`/`MeasureFormula` (syntactic `type_size` and `type_depth`) and
//! `DatatypeMeasure` (`value_depth` and `layout_size` through datatype fields).
//!
//! The load-bearing property is *legacy equivalence*: the formula-predicted measure of a
//! substitution must match the node/depth counters of the historical checked traversal (the old
//! `TypeSize`-threaded `apply_subst`) exactly, so that checked `subst` accepts and rejects
//! precisely the same instantiations it always did. We keep a small reference implementation of
//! the legacy counters here and compare against it over a family of generated type shapes.

use crate::{
    cache::{arena::ArenaBuilder, identifier_interner::IdentifierInterner},
    execution::dispatch_tables::VirtualTableKey,
    jit::execution::ast::{
        ArenaType, DatatypeMeasure, DatatypeSizes, FieldVar, FormulatedType, Type, TypeArguments,
        TypeFormula as _, TypeMeasure, TypeSizes, TypeSubst as _,
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

/// Build `TypeArguments` for tests. The through-field quantities normally come from the
/// dispatch tables; term-level substitution only reads the syntactic pair, so nominal values
/// suffice here.
fn ty_args(types: Vec<Type>) -> TypeArguments {
    TypeArguments::new(types, |ty| {
        let TypeMeasure {
            type_size,
            type_depth,
        } = ty.measure();
        Ok(TypeSizes {
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

// -------------------------------------------------------------------------------------------------
// Reference implementation of the legacy `TypeSize` counters
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
/// measurement algebra is uniform over the term structure.
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
    let base = dt(vec![
        Type::Vector(Box::new(Type::TyParam(0))),
        Type::TyParam(1),
        Type::TyParam(0),
    ]);
    // Base term: 5 nodes (S, Vector, T0, T1, T0); T0 occurs twice, deepest at depth 3;
    // T1 occurs once at depth 2.
    let arg0 = nested_vec(3); // Vector<Vector<U8>>: 3 nodes, depth 3
    let arg1 = Type::U8; // 1 node, depth 1
    assert_eq!(
        arg0.measure(),
        TypeMeasure {
            type_size: 3,
            type_depth: 3
        }
    );
    assert_eq!(
        arg1.measure(),
        TypeMeasure {
            type_size: 1,
            type_depth: 1
        }
    );

    let predicted = base
        .formula()
        .apply(&[arg0.measure(), arg1.measure()])
        .unwrap();
    // nodes = 5 + 2×3 + 1×1 = 12; depth = max(3, 3+3, 2+1) = 6.
    assert_eq!(
        predicted,
        TypeMeasure {
            type_size: 12,
            type_depth: 6
        }
    );

    // The predicted measure is well within the limits, so the substitution goes through and
    // builds exactly the expected type.
    let args = [arg0.clone(), arg1.clone()];
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

/// `TypeMeasure::check` accepts exactly at the limits and rejects just above them, with the
/// depth limit checked first.
#[test]
fn measure_check_boundaries() {
    let at_limits = TypeMeasure {
        type_size: MAX_TYPE_INSTANTIATION_NODES,
        type_depth: TYPE_DEPTH_MAX,
    };
    assert!(at_limits.check().is_ok());

    let err = TypeMeasure {
        type_size: MAX_TYPE_INSTANTIATION_NODES + 1,
        type_depth: TYPE_DEPTH_MAX,
    }
    .check()
    .unwrap_err();
    assert_eq!(err.major_status(), StatusCode::VM_MAX_TYPE_NODES_REACHED);

    let err = TypeMeasure {
        type_size: MAX_TYPE_INSTANTIATION_NODES,
        type_depth: TYPE_DEPTH_MAX + 1,
    }
    .check()
    .unwrap_err();
    assert_eq!(err.major_status(), StatusCode::VM_MAX_TYPE_DEPTH_REACHED);

    // Depth is checked first when both limits are exceeded.
    let err = TypeMeasure {
        type_size: MAX_TYPE_INSTANTIATION_NODES + 1,
        type_depth: TYPE_DEPTH_MAX + 1,
    }
    .check()
    .unwrap_err();
    assert_eq!(err.major_status(), StatusCode::VM_MAX_TYPE_DEPTH_REACHED);
}

/// The formula's prediction must equal the legacy traversal's counters on every shape, and
/// checked `subst` must accept/reject exactly on that boundary.
#[test]
fn formula_matches_legacy_traversal_counters() {
    let mut seed = 0x5EED_CAFE_u64;
    for case in 0..500u64 {
        let n_params = (case % 4) as u16 + 1;
        let base = gen_type(&mut seed, 4, n_params);
        let args = (0..n_params)
            .map(|_| gen_type(&mut seed, 3, 0))
            .collect::<Vec<_>>();

        let mut legacy = LegacyCounters::default();
        legacy_subst(&base, &args, 0, &mut legacy);

        let measures = args.iter().map(|t| t.measure()).collect::<Vec<_>>();
        let predicted = base.formula().apply(&measures).unwrap();
        assert_eq!(
            predicted.type_size, legacy.nodes,
            "nodes mismatch, case {case}"
        );
        assert_eq!(
            predicted.type_depth, legacy.max_depth,
            "depth mismatch, case {case}"
        );

        // Checked `subst` accepts or rejects exactly as the predicted measure dictates against
        // the (constant) limits.
        let result = base.subst(&args);
        if predicted.type_depth > TYPE_DEPTH_MAX {
            assert_eq!(
                result.unwrap_err().major_status(),
                StatusCode::VM_MAX_TYPE_DEPTH_REACHED,
                "case {case}"
            );
        } else if predicted.type_size > MAX_TYPE_INSTANTIATION_NODES {
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
    let base = Type::TyParam(0);
    // The parameter occurrence itself is counted in addition to the argument's nodes (legacy
    // semantics), so the predicted measure is the argument's plus one node and one level.
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

/// The precomputed-formula route (`FormulatedType`, as stored in loaded packages at translation
/// time) must agree with the on-the-fly route (`TypeSubst::subst`) on both the built type and
/// the accept/reject boundary.
#[test]
fn formulated_type_matches_on_the_fly_subst() {
    let arena = ArenaBuilder::new_bounded();
    // Vector<Vector<T0>> as both an arena term and a runtime type term.
    let arena_term = ArenaType::Vector(
        arena
            .alloc_box(ArenaType::Vector(
                arena.alloc_box(ArenaType::TyParam(0)).unwrap(),
            ))
            .unwrap(),
    );
    let type_term = Type::Vector(Box::new(Type::Vector(Box::new(Type::TyParam(0)))));

    let formulated = FormulatedType::new(arena_term, &arena).unwrap();
    let args = ty_args(vec![nested_vec(4)]);
    let arg_measures = args
        .sizes()
        .iter()
        .map(|sizes| sizes.type_measure())
        .collect::<Vec<_>>();

    // Same predicted measure...
    let predicted = formulated.predict(&args).unwrap();
    assert_eq!(predicted, type_term.formula().apply(&arg_measures).unwrap());
    // ...same built type...
    assert_eq!(
        formulated.instantiate(&args).unwrap(),
        type_term.subst(args.types()).unwrap()
    );

    // ...and the same accept/reject boundary at the constant limits. The term contributes three
    // nodes (two vectors plus the parameter occurrence) on top of the argument.
    let at_limit = ty_args(vec![nested_vec(MAX_TYPE_INSTANTIATION_NODES - 3)]);
    assert!(formulated.instantiate(&at_limit).is_ok());
    assert!(type_term.subst(at_limit.types()).is_ok());
    let over_limit = ty_args(vec![nested_vec(MAX_TYPE_INSTANTIATION_NODES - 2)]);
    let err = formulated.instantiate(&over_limit).unwrap_err();
    assert_eq!(err.major_status(), StatusCode::VM_MAX_TYPE_NODES_REACHED);
    let err = type_term.subst(over_limit.types()).unwrap_err();
    assert_eq!(err.major_status(), StatusCode::VM_MAX_TYPE_NODES_REACHED);
}

/// The datatype-level through-field measure computed at translation time: purely local field
/// structure folds into the constants, type parameters and datatype applications stay symbolic
/// terms for the dispatch tables to fold under a transaction's linkage view — and a fully
/// concrete datatype is just written down as constants.
#[test]
fn datatype_measure() {
    let arena = ArenaBuilder::new_bounded();

    // struct C { a: u64, b: vector<u32> } — fully concrete: constants, no formula.
    // value_depth = max(S→u64, S→vector→u32) = 3; layout_size = C + u64 + vector + u32 = 4.
    let fields = [
        ArenaType::U64,
        ArenaType::Vector(arena.alloc_box(ArenaType::U32).unwrap()),
    ];
    let measure = DatatypeMeasure::for_datatype_fields(fields.iter(), 0, &arena).unwrap();
    assert!(matches!(
        measure,
        DatatypeMeasure::Constant(DatatypeSizes {
            value_depth: 3,
            layout_size: 4,
        })
    ));

    // struct S<T0> { a: u64, b: vector<T0> } — a formula over T0.
    let fields = [
        ArenaType::U64,
        ArenaType::Vector(arena.alloc_box(ArenaType::TyParam(0)).unwrap()),
    ];
    let measure = DatatypeMeasure::for_datatype_fields(fields.iter(), 0, &arena).unwrap();
    let DatatypeMeasure::Formula(formula) = measure else {
        panic!("expected a formula for a generic datatype");
    };
    // value_depth(S<arg>) = max(2, 2 + value_depth(arg)) — exactly what the legacy runtime
    // `DepthFormula` derivation produced by traversing the fields (its `+1` for the datatype
    // itself is baked into the constant and offsets). layout_size(S<arg>) = 3 + layout(arg):
    // S + u64 + vector, plus the argument's layout once.
    assert_eq!(formula.value_depth_constant(), 2);
    assert_eq!(formula.layout_size_constant(), 3);
    let terms = formula.terms();
    assert_eq!(terms.len(), 1);
    assert!(matches!(terms[0].var, FieldVar::Param(0)));
    assert_eq!(terms[0].depth_offset, 2);
    assert_eq!(terms[0].occurrences, 1);

    // struct P { s: SomeOtherStruct } — the field's datatype application is a symbolic term
    // one nesting level below `P` itself, occurring once.
    let fields = [ArenaType::Datatype(dt_key())];
    let measure = DatatypeMeasure::for_datatype_fields(fields.iter(), 0, &arena).unwrap();
    let DatatypeMeasure::Formula(formula) = measure else {
        panic!("expected a formula for a datatype with datatype fields");
    };
    assert_eq!(formula.value_depth_constant(), 1);
    assert_eq!(formula.layout_size_constant(), 1);
    let terms = formula.terms();
    assert_eq!(terms.len(), 1);
    assert!(matches!(terms[0].var, FieldVar::App(_)));
    assert_eq!(terms[0].depth_offset, 1);
    assert_eq!(terms[0].occurrences, 1);

    // enum E<T0> { A { a: T0 }, B { b: u8 } } — one layout node per variant on top of the
    // datatype's own node.
    let fields = [ArenaType::TyParam(0), ArenaType::U8];
    let measure =
        DatatypeMeasure::for_datatype_fields(fields.iter(), /* variants */ 2, &arena).unwrap();
    let DatatypeMeasure::Formula(formula) = measure else {
        panic!("expected a formula for a generic enum");
    };
    assert_eq!(formula.value_depth_constant(), 2);
    assert_eq!(formula.layout_size_constant(), 4); // E + 2 variants + u8
    assert_eq!(formula.terms().len(), 1);
    assert_eq!(formula.terms()[0].depth_offset, 1);
    assert_eq!(formula.terms()[0].occurrences, 1);
}

/// `measure()` must agree with a naive node count (the semantics of the old `count_type_nodes`,
/// which `measure` subsumes).
#[test]
fn measure_matches_naive_node_count() {
    fn naive_count(ty: &Type) -> u64 {
        match ty {
            Type::Vector(t) | Type::Reference(t) | Type::MutableReference(t) => 1 + naive_count(t),
            Type::DatatypeInstantiation(inst) => 1 + inst.1.iter().map(naive_count).sum::<u64>(),
            _ => 1,
        }
    }
    let mut seed = 0xBADC_0FFE_u64;
    for _ in 0..200 {
        let ty = gen_type(&mut seed, 4, 3);
        assert_eq!(ty.measure().type_size, naive_count(&ty));
    }
}

/// The DoS shape from the type-instantiation fix: a term mentioning one parameter many times,
/// applied to a large argument. The formula rejects it by arithmetic before any part of the
/// oversized type is built.
#[test]
fn quadratic_instantiation_rejected_before_construction() {
    let base = dt(vec![Type::TyParam(0); 32]);
    let arg = nested_vec(95);

    // Predicted: 33 base nodes + 32 occurrences × 95 nodes.
    let predicted = base.formula().apply(&[arg.measure()]).unwrap();
    assert_eq!(predicted.type_size, 33 + 32 * 95);

    let err = predicted.check().unwrap_err();
    assert_eq!(err.major_status(), StatusCode::VM_MAX_TYPE_NODES_REACHED);
    let err = base.subst(&[arg]).unwrap_err();
    assert_eq!(err.major_status(), StatusCode::VM_MAX_TYPE_NODES_REACHED);
}

#[test]
fn missing_type_argument_is_an_invariant_violation() {
    let base = Type::Vector(Box::new(Type::TyParam(1)));
    let err = base
        .formula()
        .apply(&[TypeMeasure {
            type_size: 1,
            type_depth: 1,
        }])
        .unwrap_err();
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
