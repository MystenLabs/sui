// Copyright (c) The Move Contributors
// SPDX-License-Identifier: Apache-2.0

use crate::{
    expansion::ast::{Fields, ModuleIdent, Mutability, Value, Value_},
    hlir::translate::Context,
    ice, ice_assert,
    naming::ast::{self as N, BuiltinTypeName_, Type, UseFuns, Var},
    parser::ast::{DatatypeName, Field, VariantName},
    shared::{
        ast_debug::{AstDebug, AstWriter},
        matching::*,
        string_utils::debug_print,
        unique_map::UniqueMap,
    },
    typing::ast::{self as T, MatchPattern, UnannotatedPat_ as TP},
};
use move_ir_types::location::*;
use move_proc_macros::growing_stack;
use std::collections::{BTreeMap, BTreeSet, VecDeque};

//**************************************************************************************************
// Match Compilation
//**************************************************************************************************
// This mostly follows the classical Maranget (2008) implementation toward optimal decision trees.

#[derive(Debug, Clone)]
enum StructUnpack<T> {
    Default(T),
    Unpack(Vec<(Field, Var, Type)>, T),
}

#[derive(Debug, Clone)]
enum MatchTree {
    Leaf(Vec<ArmResult>),
    Failure,
    LiteralSwitch {
        subject: FringeEntry,
        subject_binders: Vec<(Mutability, Var)>,
        arms: BTreeMap<Value, Box<MatchTree>>,
        default: Box<MatchTree>, // default
    },
    StructUnpack {
        subject: FringeEntry,
        subject_binders: Vec<(Mutability, Var)>,
        tyargs: Vec<Type>,
        unpack: StructUnpack<Box<MatchTree>>,
    },
    VariantSwitch {
        subject: FringeEntry,
        subject_binders: Vec<(Mutability, Var)>,
        tyargs: Vec<Type>,
        arms: BTreeMap<VariantName, (Vec<(Field, Var, Type)>, Box<MatchTree>)>,
        default: Box<MatchTree>,
    },
}

pub(super) fn compile_match(
    context: &mut Context,
    result_type: &Type,
    subject: T::Exp,
    arms: Spanned<Vec<T::MatchArm>>,
) -> T::Exp {
    let loc = arms.loc;
    // NB: `from` also flattens `or` and converts constants into guards.
    let (pattern_matrix, arms) = PatternMatrix::from(context, loc, subject.ty.clone(), arms.value);

    let (mut initial_binders, init_subject, match_subject) =
        make_initial_fringe(context, subject, loc);

    let match_tree = build_match_tree(context, VecDeque::from([match_subject]), pattern_matrix);
    debug_print!(
        context.debug.match_translation,
        ("match tree" => match_tree; sdbg),
        ("result type" => result_type)
    );
    let mut resolution_context = ResolutionContext {
        hlir_context: context,
        output_type: result_type,
        arms: &arms,
        arms_loc: loc,
    };
    let match_exp = match_tree_to_exp(&mut resolution_context, &init_subject, match_tree);

    let eloc = match_exp.exp.loc;
    let mut seq = VecDeque::new();
    seq.append(&mut initial_binders);
    seq.push_back(sp(eloc, T::SequenceItem_::Seq(Box::new(match_exp))));
    let exp_value = sp(eloc, T::UnannotatedExp_::Block((UseFuns::new(0), seq)));
    T::exp(result_type.clone(), exp_value)
}

/// Makes the initial fringe, including the bindings, subject, and imm-ref version of the subject
/// for use in the actual decision tree.
fn make_initial_fringe(
    context: &mut Context,
    subject: T::Exp,
    loc: Loc,
) -> (VecDeque<T::SequenceItem>, FringeEntry, FringeEntry) {
    let subject_var = context.new_match_var("unpack_subject".to_string(), loc);
    let subject_loc = subject.exp.loc;
    let match_var = context.new_match_var("match_subject".to_string(), loc);

    let subject_entry = FringeEntry {
        var: subject_var,
        ty: subject.ty.clone(),
    };
    let subject_borrow_rhs = make_var_ref(subject_entry.clone());

    let match_entry = FringeEntry {
        var: match_var,
        ty: subject_borrow_rhs.ty.clone(),
    };

    let subject_binder = {
        let lhs_loc = subject_loc;
        let lhs_lvalue = make_lvalue(subject_var, Mutability::Imm, subject.ty.clone());
        let binder = T::SequenceItem_::Bind(
            sp(lhs_loc, vec![lhs_lvalue]),
            vec![Some(subject.ty.clone())],
            Box::new(subject),
        );
        sp(lhs_loc, binder)
    };

    let subject_borrow = {
        let lhs_loc = loc;
        let lhs_lvalue = make_lvalue(match_var, Mutability::Imm, subject_borrow_rhs.ty.clone());
        let binder = T::SequenceItem_::Bind(
            sp(lhs_loc, vec![lhs_lvalue]),
            vec![Some(subject_borrow_rhs.ty.clone())],
            subject_borrow_rhs,
        );
        sp(lhs_loc, binder)
    };

    (
        VecDeque::from([subject_binder, subject_borrow]),
        subject_entry,
        match_entry,
    )
}

#[growing_stack]
fn build_match_tree(
    context: &mut Context,
    mut fringe: VecDeque<FringeEntry>,
    mut matrix: PatternMatrix,
) -> MatchTree {
    debug_print!(
        context.debug.match_specialization,
        ("-----\ncompiling with fringe queue entry" => fringe; sdbg)
    );

    if matrix.is_empty() {
        debug_print!(context.debug.match_specialization, (msg "empty matrix"));
        return MatchTree::Failure;
    }

    if let Some(leaf) = matrix.wild_tree_opt(&fringe) {
        debug_print!(context.debug.match_specialization, (msg "wild leaf"), ("matrix" => matrix));
        return MatchTree::Leaf(leaf);
    }

    let Some(subject) = fringe.pop_front() else {
        debug_print!(context.debug.match_specialization, (msg "empty fringe"));
        return MatchTree::Failure;
    };

    if subject.ty.value.unfold_to_builtin_type_name().is_some() {
        compile_match_literal(context, subject, fringe, matrix)
    } else {
        let tyargs = subject.ty.value.type_arguments().unwrap().clone();

        let (mident, datatype_name) = subject
            .ty
            .value
            .unfold_to_type_name()
            .and_then(|sp!(_, name)| name.datatype_name())
            .expect("ICE non-datatype type in head constructor fringe position");

        if context.info.is_struct(&mident, &datatype_name) {
            compile_match_struct(
                context,
                subject,
                tyargs,
                fringe,
                matrix,
                mident,
                datatype_name,
            )
        } else {
            compile_variant_switch(
                context,
                subject,
                tyargs,
                fringe,
                matrix,
                mident,
                datatype_name,
            )
        }
    }
}

#[growing_stack]
fn compile_match_literal(
    context: &mut Context,
    subject: FringeEntry,
    fringe: VecDeque<FringeEntry>,
    matrix: PatternMatrix,
) -> MatchTree {
    let mut subject_binders = vec![];
    let lits = matrix.first_lits();
    let mut arms = BTreeMap::new();

    for lit in lits {
        debug_print!(context.debug.match_specialization, ("lit specializing" => lit ; fmt));
        let (mut new_binders, inner_matrix) = matrix.specialize_literal(&lit);
        subject_binders.append(&mut new_binders);
        arms.insert(
            lit,
            Box::new(build_match_tree(context, fringe.clone(), inner_matrix)),
        );
    }

    let (mut new_binders, default) = matrix.specialize_default();
    subject_binders.append(&mut new_binders);
    let default_result = Box::new(build_match_tree(context, fringe, default));

    MatchTree::LiteralSwitch {
        subject,
        subject_binders,
        arms,
        default: default_result,
    }
}

#[growing_stack]
fn compile_match_struct(
    context: &mut Context,
    subject: FringeEntry,
    tyargs: Vec<Type>,
    fringe: VecDeque<FringeEntry>,
    matrix: PatternMatrix,
    mident: ModuleIdent,
    datatype_name: DatatypeName,
) -> MatchTree {
    let decl_fields = context.info.struct_fields(&mident, &datatype_name).unwrap();
    let (subject_binders, unpack) = if let Some((ploc, arg_types)) = matrix.first_struct_ctors() {
        let fringe_binders = context.make_imm_ref_match_binders(decl_fields, ploc, arg_types);
        let fringe_exps = make_fringe_entries(&fringe_binders);
        let inner_fringe = fringe_exps.into_iter().chain(fringe.clone()).collect();

        let bind_tys = fringe_binders
            .iter()
            .map(|(_, _, ty)| ty)
            .collect::<Vec<_>>();
        let (subject_binders, inner_matrix) = matrix.specialize_struct(context, bind_tys);

        let unpack = StructUnpack::Unpack(
            fringe_binders,
            Box::new(build_match_tree(context, inner_fringe, inner_matrix)),
        );
        (subject_binders, unpack)
    } else {
        let (subject_binders, default_matrix) = matrix.specialize_default();
        let unpack =
            StructUnpack::Default(Box::new(build_match_tree(context, fringe, default_matrix)));
        (subject_binders, unpack)
    };

    MatchTree::StructUnpack {
        subject,
        subject_binders,
        tyargs,
        unpack,
    }
}

#[growing_stack]
fn compile_variant_switch(
    context: &mut Context,
    subject: FringeEntry,
    tyargs: Vec<Type>,
    fringe: VecDeque<FringeEntry>,
    matrix: PatternMatrix,
    mident: ModuleIdent,
    datatype_name: DatatypeName,
) -> MatchTree {
    let mut subject_binders = vec![];
    let mut unmatched_variants = context
        .info
        .enum_variants(&mident, &datatype_name)
        .into_iter()
        .collect::<BTreeSet<_>>();

    let ctors = matrix.first_variant_ctors();
    let mut arms = BTreeMap::new();

    for (ctor, (ploc, arg_types)) in ctors {
        unmatched_variants.remove(&ctor);
        let decl_fields = context
            .info
            .enum_variant_fields(&mident, &datatype_name, &ctor)
            .unwrap();
        let fringe_binders = context.make_imm_ref_match_binders(decl_fields, ploc, arg_types);
        let fringe_exps = make_fringe_entries(&fringe_binders);
        let inner_fringe = fringe_exps.into_iter().chain(fringe.clone()).collect();

        let bind_tys = fringe_binders
            .iter()
            .map(|(_, _, ty)| ty)
            .collect::<Vec<_>>();
        let (mut new_binders, inner_matrix) = matrix.specialize_variant(context, &ctor, bind_tys);
        subject_binders.append(&mut new_binders);

        arms.insert(
            ctor,
            (
                fringe_binders,
                Box::new(build_match_tree(context, inner_fringe, inner_matrix)),
            ),
        );
    }

    let (mut new_binders, default_matrix) = if unmatched_variants.is_empty() {
        let empty_pattern = PatternMatrix {
            tys: vec![],
            loc: matrix.loc,
            patterns: vec![],
        };
        (vec![], empty_pattern)
    } else {
        matrix.specialize_default()
    };
    subject_binders.append(&mut new_binders);

    MatchTree::VariantSwitch {
        subject,
        subject_binders,
        tyargs,
        arms,
        default: Box::new(build_match_tree(context, fringe, default_matrix)),
    }
}

fn make_fringe_entries(binders: &[(Field, Var, Type)]) -> VecDeque<FringeEntry> {
    binders
        .iter()
        .map(|(_, var, ty)| FringeEntry {
            var: *var,
            ty: ty.clone(),
        })
        .collect::<VecDeque<_>>()
}

//------------------------------------------------
// Result Construction
//------------------------------------------------

struct ResolutionContext<'ctxt, 'call> {
    hlir_context: &'call mut Context<'ctxt>,
    output_type: &'call Type,
    arms: &'call Vec<T::Exp>,
    arms_loc: Loc,
}

impl<'ctxt, 'call> ResolutionContext<'ctxt, 'call> {
    fn arm(&self, index: usize) -> T::Exp {
        self.arms[index].clone()
    }

    fn arms_loc(&self) -> Loc {
        self.arms_loc
    }

    fn output_type(&self) -> Type {
        self.output_type.clone()
    }
}

#[growing_stack]
fn match_tree_to_exp(
    context: &mut ResolutionContext,
    init_subject: &FringeEntry,
    result: MatchTree,
) -> T::Exp {
    match result {
        MatchTree::Leaf(leaf) => make_leaf(context, init_subject, leaf),
        MatchTree::Failure => {
            context.hlir_context.add_diag(ice!((context.arms_loc, "Generated a failure expression, which should not be allowed under match exhaustion.")));
            T::exp(
                context.output_type(),
                sp(context.arms_loc, T::UnannotatedExp_::UnresolvedError),
            )
        }
        MatchTree::VariantSwitch {
            subject,
            subject_binders,
            tyargs,
            mut arms,
            default,
        } => {
            let (m, e) = subject
                .ty
                .value
                .unfold_to_type_name()
                .and_then(|sp!(_, name)| name.datatype_name())
                .unwrap();
            // Bindings in the arm are always immutable
            let bindings = subject_binders
                .into_iter()
                .map(|(_mut, binder)| (binder, (Mutability::Imm, subject.clone())))
                .collect();

            let sorted_variants: Vec<VariantName> = context.hlir_context.info.enum_variants(&m, &e);
            let mut blocks = vec![];
            for v in sorted_variants {
                if let Some((unpack_fields, next)) = arms.remove(&v) {
                    let rest_result = match_tree_to_exp(context, init_subject, *next);
                    let unpack_block = make_match_variant_unpack(
                        context,
                        m,
                        e,
                        v,
                        tyargs.clone(),
                        unpack_fields,
                        subject.clone(),
                        rest_result,
                    );
                    blocks.push((v, unpack_block));
                } else {
                    let default_tree = (*default).clone();
                    let rest_result = match_tree_to_exp(context, init_subject, default_tree);
                    blocks.push((v, rest_result));
                }
            }
            let out_exp = T::UnannotatedExp_::VariantMatch(make_var_ref(subject), (m, e), blocks);
            let body_exp = T::exp(context.output_type(), sp(context.arms_loc(), out_exp));
            make_copy_bindings(context, bindings, body_exp)
        }
        MatchTree::StructUnpack {
            subject,
            subject_binders,
            tyargs,
            unpack,
        } => {
            let (m, s) = subject
                .ty
                .value
                .unfold_to_type_name()
                .and_then(|sp!(_, name)| name.datatype_name())
                .unwrap();
            // Bindings in the arm are always immutable
            let bindings = subject_binders
                .into_iter()
                .map(|(_mut, binder)| (binder, (Mutability::Imm, subject.clone())))
                .collect();
            let unpack_exp = match unpack {
                StructUnpack::Default(next) => match_tree_to_exp(context, init_subject, *next),
                StructUnpack::Unpack(unpack_fields, next) => {
                    let rest_result = match_tree_to_exp(context, init_subject, *next);
                    make_match_struct_unpack(
                        context,
                        m,
                        s,
                        tyargs.clone(),
                        unpack_fields,
                        subject.clone(),
                        rest_result,
                    )
                }
            };
            make_copy_bindings(context, bindings, unpack_exp)
        }
        MatchTree::LiteralSwitch {
            subject,
            subject_binders,
            mut arms,
            default: _,
        } if matches!(
            subject.ty.value.unfold_to_builtin_type_name(),
            Some(sp!(_, BuiltinTypeName_::Bool))
        ) && arms.len() == 2 =>
        {
            // Bindings in the arm are always immutable
            let bindings = subject_binders
                .into_iter()
                .map(|(_mut, binder)| (binder, (Mutability::Imm, subject.clone())))
                .collect();
            // If the literal switch for a boolean is saturated, no default case.
            let lit_subject = make_match_lit(subject.clone());
            let true_arm = arms
                .remove(&sp(Loc::invalid(), Value_::Bool(true)))
                .unwrap();
            let false_arm = arms
                .remove(&sp(Loc::invalid(), Value_::Bool(false)))
                .unwrap();

            let true_arm = match_tree_to_exp(context, init_subject, *true_arm);
            let false_arm = match_tree_to_exp(context, init_subject, *false_arm);

            make_copy_bindings(
                context,
                bindings,
                make_if_else_arm(context, lit_subject, true_arm, false_arm),
            )
        }
        MatchTree::LiteralSwitch {
            subject,
            subject_binders,
            arms: map,
            default,
        } => {
            // Bindings in the arm are always immutable
            let bindings = subject_binders
                .into_iter()
                .map(|(_mut, binder)| (binder, (Mutability::Imm, subject.clone())))
                .collect();
            let lit_subject = make_match_lit(subject.clone());

            let mut entries = map.into_iter().collect::<Vec<_>>();
            entries.sort_by(|(key1, _), (key2, _)| key1.cmp(key2));

            let else_work_result = (*default).clone();
            let mut out_exp = match_tree_to_exp(context, init_subject, else_work_result);

            for (key, next_tree) in entries.into_iter().rev() {
                let match_arm = match_tree_to_exp(context, init_subject, *next_tree);
                let test_exp = make_lit_test(lit_subject.clone(), key);
                out_exp = make_if_else_arm(context, test_exp, match_arm, out_exp);
            }
            make_copy_bindings(context, bindings, out_exp)
        }
    }
}

fn make_leaf(
    context: &mut ResolutionContext,
    subject: &FringeEntry,
    mut leaf: Vec<ArmResult>,
) -> T::Exp {
    assert!(!leaf.is_empty(), "ICE empty leaf in matching");

    if leaf.len() == 1 {
        let last = leaf.pop().unwrap();
        ice_assert!(
            context.hlir_context.reporter,
            last.guard.is_none(),
            last.guard.unwrap().exp.loc,
            "Must have a non-guarded leaf"
        );
        let arm = make_arm(context, subject.clone(), last.arm);
        return make_copy_bindings(context, last.bindings, arm);
    }

    let last = leaf.pop().unwrap();
    ice_assert!(
        context.hlir_context.reporter,
        last.guard.is_none(),
        last.guard.unwrap().exp.loc,
        "Must have a non-guarded leaf"
    );
    let arm = make_arm(context, subject.clone(), last.arm);
    let mut out_exp = make_copy_bindings(context, last.bindings, arm);
    while let Some(arm) = leaf.pop() {
        ice_assert!(
            context.hlir_context.reporter,
            arm.guard.is_some(),
            arm.loc,
            "Expected a guard"
        );
        out_exp = make_guard_exp(context, subject, arm, out_exp);
    }
    out_exp
}

fn make_guard_exp(
    context: &mut ResolutionContext,
    subject: &FringeEntry,
    arm: ArmResult,
    cur_exp: T::Exp,
) -> T::Exp {
    let ArmResult {
        loc: _,
        bindings,
        guard,
        arm,
    } = arm;
    // Bindings in the guard are always immutable
    let bindings = bindings
        .into_iter()
        .map(|(x, (_mut, entry))| (x, (Mutability::Imm, entry)))
        .collect();
    let guard_arm = make_arm(context, subject.clone(), arm);
    let body = make_if_else_arm(context, *guard.unwrap(), guard_arm, cur_exp);
    make_copy_bindings(context, bindings, body)
}

fn make_arm(context: &mut ResolutionContext, subject: FringeEntry, arm: Arm) -> T::Exp {
    let arm_exp = context.arm(arm.index);
    make_arm_unpack(
        context,
        subject,
        arm.orig_pattern,
        &arm.rhs_binders,
        arm_exp,
    )
}

fn make_arm_unpack(
    context: &mut ResolutionContext,
    subject: FringeEntry,
    pattern: MatchPattern,
    rhs_binders: &BTreeSet<Var>,
    next: T::Exp,
) -> T::Exp {
    let ploc = pattern.pat.loc;
    let mut seq = VecDeque::new();

    let mut queue: VecDeque<(FringeEntry, MatchPattern)> = VecDeque::from([(subject, pattern)]);

    // TODO(cgswords): we can coalese patterns a bit here, but don't for now.
    while let Some((entry, pat)) = queue.pop_front() {
        let ploc = pat.pat.loc;
        match pat.pat.value {
            TP::Variant(m, e, v, tys, fs) => {
                let Some((queue_entries, unpack)) =
                    arm_variant_unpack(context, None, ploc, m, e, tys, v, fs, entry)
                else {
                    context.hlir_context.add_diag(ice!((
                        ploc,
                        "Did not build an arm unpack for a value variant"
                    )));
                    continue;
                };
                for entry in queue_entries.into_iter().rev() {
                    queue.push_front(entry);
                }
                seq.push_back(unpack);
            }
            TP::BorrowVariant(mut_, m, e, v, tys, fs) => {
                let Some((queue_entries, unpack)) =
                    arm_variant_unpack(context, Some(mut_), ploc, m, e, tys, v, fs, entry)
                else {
                    continue;
                };
                for entry in queue_entries.into_iter().rev() {
                    queue.push_front(entry);
                }
                seq.push_back(unpack);
            }
            TP::Struct(m, s, tys, fs) => {
                let Some((queue_entries, unpack)) =
                    arm_struct_unpack(context, None, ploc, m, s, tys, fs, entry)
                else {
                    context.hlir_context.add_diag(ice!((
                        ploc,
                        "Did not build an arm unpack for a value struct"
                    )));
                    continue;
                };
                for entry in queue_entries.into_iter().rev() {
                    queue.push_front(entry);
                }
                seq.push_back(unpack);
            }
            TP::BorrowStruct(mut_, m, s, tys, fs) => {
                let Some((queue_entries, unpack)) =
                    arm_struct_unpack(context, Some(mut_), ploc, m, s, tys, fs, entry)
                else {
                    continue;
                };
                for entry in queue_entries.into_iter().rev() {
                    queue.push_front(entry);
                }
                seq.push_back(unpack);
            }
            TP::Literal(_) => (),
            TP::Binder(mut_, x) if rhs_binders.contains(&x) => {
                seq.push_back(make_move_binding(x, mut_, entry.ty.clone(), entry))
            }
            TP::Binder(_, _) => (),
            TP::Wildcard => (),
            TP::At(x, inner) => {
                // See comment in typing/translate.rs at pattern typing for more information.
                let x_in_rhs_binders = rhs_binders.contains(&x);
                let inner_has_rhs_binders = match_pattern_has_binders(&inner, rhs_binders);
                match (x_in_rhs_binders, inner_has_rhs_binders) {
                    // make a copy of the value (or ref) and do both sides
                    (true, true) => {
                        let bind_entry = entry.clone();
                        seq.push_back(make_copy_binding(
                            x,
                            Mutability::Imm,
                            bind_entry.ty.clone(),
                            bind_entry,
                        ));
                        queue.push_front((entry, *inner));
                    }
                    // no unpack needed, just move the value to the x
                    (true, false) => seq.push_back(make_move_binding(
                        x,
                        Mutability::Imm,
                        entry.ty.clone(),
                        entry,
                    )),
                    // we need to unpack either way, handling wildcards and the like
                    (false, _) => queue.push_front((entry, *inner)),
                }
            }
            TP::ErrorPat => (),
            TP::Constant(_, _) | TP::Or(_, _) => unreachable!(),
        }
    }

    let nloc = next.exp.loc;
    seq.push_back(sp(nloc, T::SequenceItem_::Seq(Box::new(next))));

    let body = T::UnannotatedExp_::Block((UseFuns::new(0), seq));
    T::exp(context.output_type(), sp(ploc, body))
}

fn match_pattern_has_binders(pat: &T::MatchPattern, rhs_binders: &BTreeSet<Var>) -> bool {
    match &pat.pat.value {
        TP::Binder(_, x) => rhs_binders.contains(x),
        TP::At(x, inner) => {
            rhs_binders.contains(x) || match_pattern_has_binders(inner, rhs_binders)
        }
        TP::Variant(_, _, _, _, fields) | TP::BorrowVariant(_, _, _, _, _, fields) => fields
            .iter()
            .any(|(_, _, (_, (_, pat)))| match_pattern_has_binders(pat, rhs_binders)),
        TP::Struct(_, _, _, fields) | TP::BorrowStruct(_, _, _, _, fields) => fields
            .iter()
            .any(|(_, _, (_, (_, pat)))| match_pattern_has_binders(pat, rhs_binders)),
        TP::Literal(_) => false,
        TP::Wildcard => false,
        TP::ErrorPat => false,
        TP::Constant(_, _) | TP::Or(_, _) => unreachable!(),
    }
}

fn arm_variant_unpack(
    context: &mut ResolutionContext,
    mut_ref: Option<bool>,
    pat_loc: Loc,
    mident: ModuleIdent,
    enum_: DatatypeName,
    tyargs: Vec<Type>,
    variant: VariantName,
    fields: Fields<(Type, MatchPattern)>,
    rhs: FringeEntry,
) -> Option<(Vec<(FringeEntry, MatchPattern)>, T::SequenceItem)> {
    let all_wild = fields
        .iter()
        .all(|(_, _, (_, (_, pat)))| matches!(pat.pat.value, TP::Wildcard))
        || fields.is_empty();
    // If we are  matching a  ref with no fields under it, we aren't going to drop so
    // we just continue on.
    if all_wild && mut_ref.is_some() {
        return None;
    }

    let (queue_entries, fields) =
        make_arm_variant_unpack_fields(context, mut_ref, pat_loc, mident, enum_, variant, fields);
    let unpack = make_arm_variant_unpack_stmt(mut_ref, mident, enum_, variant, tyargs, fields, rhs);
    Some((queue_entries, unpack))
}

fn arm_struct_unpack(
    context: &mut ResolutionContext,
    mut_ref: Option<bool>,
    pat_loc: Loc,
    mident: ModuleIdent,
    struct_: DatatypeName,
    tyargs: Vec<Type>,
    fields: Fields<(Type, MatchPattern)>,
    rhs: FringeEntry,
) -> Option<(Vec<(FringeEntry, MatchPattern)>, T::SequenceItem)> {
    let all_wild = fields
        .iter()
        .all(|(_, _, (_, (_, pat)))| matches!(pat.pat.value, TP::Wildcard))
        || fields.is_empty();
    // If we are  matching a  ref with no fields under it, we aren't going to drop so
    // we just continue on.
    if all_wild && mut_ref.is_some() {
        return None;
    }

    let (queue_entries, fields) =
        make_arm_struct_unpack_fields(context, mut_ref, pat_loc, mident, struct_, fields);
    let unpack = make_arm_struct_unpack_stmt(mut_ref, mident, struct_, tyargs, fields, rhs);
    Some((queue_entries, unpack))
}

//------------------------------------------------
// Unpack Field Builders
//------------------------------------------------

fn make_arm_variant_unpack_fields(
    context: &mut ResolutionContext,
    mut_ref: Option<bool>,
    pat_loc: Loc,
    mident: ModuleIdent,
    enum_: DatatypeName,
    variant: VariantName,
    fields: Fields<(Type, MatchPattern)>,
) -> (Vec<(FringeEntry, MatchPattern)>, Vec<(Field, Var, Type)>) {
    let field_pats = fields.clone().map(|_key, (ndx, (_, pat))| (ndx, pat));

    let decl_fields = context
        .hlir_context
        .info
        .enum_variant_fields(&mident, &enum_, &variant)
        .unwrap();

    let field_tys = {
        let field_tys = fields.map(|_key, (ndx, (ty, _))| (ndx, ty));
        if let Some(mut_) = mut_ref {
            field_tys.map(|_field, (ndx, sp!(loc, ty))| {
                (
                    ndx,
                    sp(loc, N::Type_::Ref(mut_, Box::new(sp(loc, ty.base_type_())))),
                )
            })
        } else {
            field_tys
        }
    };
    let fringe_binders =
        context
            .hlir_context
            .make_unpack_binders(decl_fields.clone(), pat_loc, field_tys);
    let fringe_exps = make_fringe_entries(&fringe_binders);

    let ordered_pats = order_fields_by_decl(decl_fields, field_pats);

    let mut unpack_fields: Vec<(Field, Var, Type)> = vec![];
    assert!(fringe_exps.len() == ordered_pats.len());
    for (fringe_exp, (_, field, _)) in fringe_exps.iter().zip(ordered_pats.iter()) {
        unpack_fields.push((*field, fringe_exp.var, fringe_exp.ty.clone()));
    }
    let queue_entries = fringe_exps
        .into_iter()
        .zip(
            ordered_pats
                .into_iter()
                .map(|(_, _, ordered_pat)| ordered_pat),
        )
        .collect::<Vec<_>>();

    (queue_entries, unpack_fields)
}

fn make_arm_struct_unpack_fields(
    context: &mut ResolutionContext,
    mut_ref: Option<bool>,
    pat_loc: Loc,
    mident: ModuleIdent,
    struct_: DatatypeName,
    fields: Fields<(Type, MatchPattern)>,
) -> (Vec<(FringeEntry, MatchPattern)>, Vec<(Field, Var, Type)>) {
    let field_pats = fields.clone().map(|_key, (ndx, (_, pat))| (ndx, pat));
    let decl_fields = context
        .hlir_context
        .info
        .struct_fields(&mident, &struct_)
        .unwrap();

    let field_tys = {
        let field_tys = fields.map(|_key, (ndx, (ty, _))| (ndx, ty));
        if let Some(mut_) = mut_ref {
            field_tys.map(|_field, (ndx, sp!(loc, ty))| {
                (
                    ndx,
                    sp(loc, N::Type_::Ref(mut_, Box::new(sp(loc, ty.base_type_())))),
                )
            })
        } else {
            field_tys
        }
    };
    let fringe_binders =
        context
            .hlir_context
            .make_unpack_binders(decl_fields.clone(), pat_loc, field_tys);
    let fringe_exps = make_fringe_entries(&fringe_binders);

    let ordered_pats = order_fields_by_decl(decl_fields, field_pats);

    let mut unpack_fields: Vec<(Field, Var, Type)> = vec![];
    assert!(fringe_exps.len() == ordered_pats.len());
    for (fringe_exp, (_, field, _)) in fringe_exps.iter().zip(ordered_pats.iter()) {
        unpack_fields.push((*field, fringe_exp.var, fringe_exp.ty.clone()));
    }
    let queue_entries = fringe_exps
        .into_iter()
        .zip(
            ordered_pats
                .into_iter()
                .map(|(_, _, ordered_pat)| ordered_pat),
        )
        .collect::<Vec<_>>();

    (queue_entries, unpack_fields)
}

//------------------------------------------------
// Expression Creation Helpers
//------------------------------------------------

fn make_var_ref(subject: FringeEntry) -> Box<T::Exp> {
    let FringeEntry { var, ty } = subject;
    match ty {
        sp!(_, N::Type_::Ref(false, _)) => {
            let loc = var.loc;
            Box::new(make_copy_exp(ty, loc, var))
        }
        sp!(_, N::Type_::Ref(true, inner)) => {
            // NB(cswords): we freeze the mut ref at the non-mut ref type.
            let loc = var.loc;
            let ref_ty = sp(loc, N::Type_::Ref(true, inner.clone()));
            let freeze_arg = make_copy_exp(ref_ty, loc, var);
            let freeze_ty = sp(loc, N::Type_::Ref(false, inner));
            Box::new(make_freeze_exp(freeze_ty, loc, freeze_arg))
        }
        ty => {
            // NB(cswords): we borrow the local
            let loc = var.loc;
            let ref_ty = sp(loc, N::Type_::Ref(false, Box::new(ty)));
            let borrow_exp = T::UnannotatedExp_::BorrowLocal(false, var);
            Box::new(T::exp(ref_ty, sp(loc, borrow_exp)))
        }
    }
}

// Performs an unpack for the purpose of matching, where we are matching against an imm. ref.
fn make_match_variant_unpack(
    context: &ResolutionContext,
    mident: ModuleIdent,
    enum_: DatatypeName,
    variant: VariantName,
    tyargs: Vec<Type>,
    fields: Vec<(Field, Var, Type)>,
    rhs: FringeEntry,
    next: T::Exp,
) -> T::Exp {
    assert!(matches!(rhs.ty.value, N::Type_::Ref(false, _)));
    let mut seq = VecDeque::new();

    let rhs_loc = rhs.var.loc;
    let mut lvalue_fields: Fields<(Type, T::LValue)> = UniqueMap::new();

    for (ndx, (field_name, var, ty)) in fields.into_iter().enumerate() {
        assert!(ty.value.is_ref().is_some());
        let var_lvalue = make_lvalue(var, Mutability::Imm, ty.clone());
        let lhs_ty = sp(ty.loc, ty.value.base_type_());
        lvalue_fields
            .add(field_name, (ndx, (lhs_ty, var_lvalue)))
            .unwrap();
    }

    let unpack_lvalue = sp(
        rhs_loc,
        T::LValue_::BorrowUnpackVariant(false, mident, enum_, variant, tyargs, lvalue_fields),
    );

    let FringeEntry { var, ty } = rhs;
    let rhs = Box::new(make_copy_exp(ty.clone(), var.loc, var));
    let binder = T::SequenceItem_::Bind(sp(rhs_loc, vec![unpack_lvalue]), vec![Some(ty)], rhs);
    seq.push_back(sp(rhs_loc, binder));

    let eloc = next.exp.loc;
    seq.push_back(sp(eloc, T::SequenceItem_::Seq(Box::new(next))));

    let exp_value = sp(eloc, T::UnannotatedExp_::Block((UseFuns::new(0), seq)));
    T::exp(context.output_type(), exp_value)
}

// Performs a struct unpack for the purpose of matching, where we are matching against an imm. ref.
fn make_match_struct_unpack(
    context: &ResolutionContext,
    mident: ModuleIdent,
    struct_: DatatypeName,
    tyargs: Vec<Type>,
    fields: Vec<(Field, Var, Type)>,
    rhs: FringeEntry,
    next: T::Exp,
) -> T::Exp {
    assert!(matches!(rhs.ty.value, N::Type_::Ref(false, _)));
    let mut seq = VecDeque::new();

    let rhs_loc = rhs.var.loc;
    let mut lvalue_fields: Fields<(Type, T::LValue)> = UniqueMap::new();

    for (ndx, (field_name, var, ty)) in fields.into_iter().enumerate() {
        assert!(ty.value.is_ref().is_some());
        let var_lvalue = make_lvalue(var, Mutability::Imm, ty.clone());
        let lhs_ty = sp(ty.loc, ty.value.base_type_());
        lvalue_fields
            .add(field_name, (ndx, (lhs_ty, var_lvalue)))
            .unwrap();
    }

    let unpack_lvalue = sp(
        rhs_loc,
        T::LValue_::BorrowUnpack(false, mident, struct_, tyargs, lvalue_fields),
    );

    let FringeEntry { var, ty } = rhs;
    let rhs = Box::new(make_copy_exp(ty.clone(), var.loc, var));
    let binder = T::SequenceItem_::Bind(sp(rhs_loc, vec![unpack_lvalue]), vec![Some(ty)], rhs);
    seq.push_back(sp(rhs_loc, binder));

    let eloc = next.exp.loc;
    seq.push_back(sp(eloc, T::SequenceItem_::Seq(Box::new(next))));

    let exp_value = sp(eloc, T::UnannotatedExp_::Block((UseFuns::new(0), seq)));
    T::exp(context.output_type(), exp_value)
}

fn make_arm_variant_unpack_stmt(
    mut_ref: Option<bool>,
    mident: ModuleIdent,
    enum_: DatatypeName,
    variant: VariantName,
    tyargs: Vec<Type>,
    fields: Vec<(Field, Var, Type)>,
    rhs: FringeEntry,
) -> T::SequenceItem {
    let rhs_loc = rhs.var.loc;
    let mut lvalue_fields: Fields<(Type, T::LValue)> = UniqueMap::new();

    for (ndx, (field_name, var, ty)) in fields.into_iter().enumerate() {
        let var_lvalue = make_lvalue(var, Mutability::Imm, ty.clone());
        let lhs_ty = sp(ty.loc, ty.value.base_type_());
        lvalue_fields
            .add(field_name, (ndx, (lhs_ty, var_lvalue)))
            .unwrap();
    }

    let unpack_lvalue_ = if let Some(mut_) = mut_ref {
        T::LValue_::BorrowUnpackVariant(mut_, mident, enum_, variant, tyargs, lvalue_fields)
    } else {
        T::LValue_::UnpackVariant(mident, enum_, variant, tyargs, lvalue_fields)
    };
    let rhs_ty = rhs.ty.clone();
    let rhs: Box<T::Exp> = Box::new(rhs.into_move_exp());
    let binder = T::SequenceItem_::Bind(
        sp(rhs_loc, vec![sp(rhs_loc, unpack_lvalue_)]),
        vec![Some(rhs_ty)],
        rhs,
    );
    sp(rhs_loc, binder)
}

fn make_arm_struct_unpack_stmt(
    mut_ref: Option<bool>,
    mident: ModuleIdent,
    struct_: DatatypeName,
    tyargs: Vec<Type>,
    fields: Vec<(Field, Var, Type)>,
    rhs: FringeEntry,
) -> T::SequenceItem {
    let rhs_loc = rhs.var.loc;
    let mut lvalue_fields: Fields<(Type, T::LValue)> = UniqueMap::new();

    for (ndx, (field_name, var, ty)) in fields.into_iter().enumerate() {
        let var_lvalue = make_lvalue(var, Mutability::Imm, ty.clone());
        let lhs_ty = sp(ty.loc, ty.value.base_type_());
        lvalue_fields
            .add(field_name, (ndx, (lhs_ty, var_lvalue)))
            .unwrap();
    }

    let unpack_lvalue_ = if let Some(mut_) = mut_ref {
        T::LValue_::BorrowUnpack(mut_, mident, struct_, tyargs, lvalue_fields)
    } else {
        T::LValue_::Unpack(mident, struct_, tyargs, lvalue_fields)
    };
    let rhs_ty = rhs.ty.clone();
    let rhs: Box<T::Exp> = Box::new(rhs.into_move_exp());
    let binder = T::SequenceItem_::Bind(
        sp(rhs_loc, vec![sp(rhs_loc, unpack_lvalue_)]),
        vec![Some(rhs_ty)],
        rhs,
    );
    sp(rhs_loc, binder)
}

fn make_match_lit(subject: FringeEntry) -> T::Exp {
    let FringeEntry { var, ty } = subject;
    match ty {
        sp!(ty_loc, N::Type_::Ref(false, inner)) => {
            let loc = var.loc;
            let copy_exp = make_copy_exp(sp(ty_loc, N::Type_::Ref(false, inner.clone())), loc, var);
            make_deref_exp(*inner, loc, copy_exp)
        }
        sp!(_, N::Type_::Ref(true, inner)) => {
            let loc = var.loc;

            // NB(cswords): we now freeze the mut ref at the non-mut ref type.
            let ref_ty = sp(loc, N::Type_::Ref(true, inner.clone()));
            let freeze_arg = make_copy_exp(ref_ty, loc, var);
            let freeze_ty = sp(loc, N::Type_::Ref(false, inner.clone()));
            let frozen_exp = make_freeze_exp(freeze_ty, loc, freeze_arg);
            make_deref_exp(*inner, loc, frozen_exp)
        }
        _ty => unreachable!(),
    }
}

fn make_copy_bindings(context: &ResolutionContext, bindings: PatBindings, next: T::Exp) -> T::Exp {
    make_bindings(context, bindings, next, true)
}

fn make_bindings(
    context: &ResolutionContext,
    bindings: PatBindings,
    next: T::Exp,
    as_copy: bool,
) -> T::Exp {
    let eloc = next.exp.loc;
    let mut seq = VecDeque::new();
    for (lhs, (mut_, rhs)) in bindings {
        let binding = if as_copy {
            make_copy_binding(lhs, mut_, rhs.ty.clone(), rhs)
        } else {
            make_move_binding(lhs, mut_, rhs.ty.clone(), rhs)
        };
        seq.push_back(binding);
    }
    seq.push_back(sp(eloc, T::SequenceItem_::Seq(Box::new(next))));
    let exp_value = sp(eloc, T::UnannotatedExp_::Block((UseFuns::new(0), seq)));
    T::exp(context.output_type(), exp_value)
}

fn make_lvalue(lhs: Var, mut_: Mutability, ty: Type) -> T::LValue {
    let lhs_loc = lhs.loc;
    let lhs_var = T::LValue_::Var {
        var: lhs,
        ty: Box::new(ty.clone()),
        mut_: Some(mut_),
        unused_binding: false,
    };
    sp(lhs_loc, lhs_var)
}

fn make_move_binding(lhs: Var, mut_: Mutability, ty: Type, rhs: FringeEntry) -> T::SequenceItem {
    let lhs_loc = lhs.loc;
    let lhs_lvalue = make_lvalue(lhs, mut_, ty.clone());
    let binder = T::SequenceItem_::Bind(
        sp(lhs_loc, vec![lhs_lvalue]),
        vec![Some(ty)],
        Box::new(rhs.into_move_exp()),
    );
    sp(lhs_loc, binder)
}

fn make_copy_binding(lhs: Var, mut_: Mutability, ty: Type, rhs: FringeEntry) -> T::SequenceItem {
    let lhs_loc = lhs.loc;
    let lhs_lvalue = make_lvalue(lhs, mut_, ty.clone());
    let binder = T::SequenceItem_::Bind(
        sp(lhs_loc, vec![lhs_lvalue]),
        vec![Some(ty.clone())],
        Box::new(make_copy_exp(ty, rhs.var.loc, rhs.var)),
    );
    sp(lhs_loc, binder)
}

fn make_lit_test(lit_exp: T::Exp, value: Value) -> T::Exp {
    let loc = value.loc;
    let value_exp = T::exp(
        lit_exp.ty.clone(),
        sp(loc, T::UnannotatedExp_::Value(value)),
    );
    make_eq_test(loc, lit_exp, value_exp)
}

fn make_if_else_arm(
    context: &ResolutionContext,
    test: T::Exp,
    conseq: T::Exp,
    alt: T::Exp,
) -> T::Exp {
    // FIXME: this span is woefully wrong
    let loc = test.exp.loc;
    T::exp(
        context.output_type(),
        sp(
            loc,
            T::UnannotatedExp_::IfElse(Box::new(test), Box::new(conseq), Some(Box::new(alt))),
        ),
    )
}

fn make_copy_exp(ty: Type, loc: Loc, var: Var) -> T::Exp {
    let exp_ = T::UnannotatedExp_::Copy {
        var,
        from_user: false,
    };
    T::exp(ty, sp(loc, exp_))
}

fn make_freeze_exp(ty: Type, loc: Loc, arg: T::Exp) -> T::Exp {
    let freeze_fn = Box::new(sp(loc, T::BuiltinFunction_::Freeze(ty.clone())));
    let freeze_exp = T::UnannotatedExp_::Builtin(freeze_fn, Box::new(arg));
    T::exp(ty, sp(loc, freeze_exp))
}

fn make_deref_exp(ty: Type, loc: Loc, arg: T::Exp) -> T::Exp {
    let deref_exp = T::UnannotatedExp_::Dereference(Box::new(arg));
    T::exp(ty, sp(loc, deref_exp))
}

//**************************************************************************************************
// Debug Print
//**************************************************************************************************

impl AstDebug for PatternMatrix {
    fn ast_debug(&self, w: &mut AstWriter) {
        for arm in &self.patterns {
            let PatternArm {
                pats: pat,
                guard,
                arm,
            } = arm;
            w.write("    { ");
            w.comma(pat, |w, p| p.ast_debug(w));
            w.write(" } =>");
            if let Some(guard) = guard {
                w.write(" if ");
                guard.ast_debug(w);
            }
            w.write(" [");
            w.write(format!("] arm {}\n", arm.index));
        }
    }
}

impl AstDebug for FringeEntry {
    fn ast_debug(&self, w: &mut AstWriter) {
        w.write(format!("{:?} : ", self.var));
        self.ty.ast_debug(w);
    }
}
