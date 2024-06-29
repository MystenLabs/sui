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

type Fringe = VecDeque<FringeEntry>;

#[derive(Clone)]
enum StructUnpack<T> {
    Default(T),
    Unpack(Vec<(Field, Var, Type)>, T),
}

enum MatchStep {
    Leaf(Vec<ArmResult>),
    Failure,
    LiteralSwitch {
        subject: FringeEntry,
        subject_binders: Vec<(Mutability, Var)>,
        fringe: Fringe,
        arms: BTreeMap<Value, PatternMatrix>,
        default: PatternMatrix,
    },
    StructUnpack {
        subject: FringeEntry,
        subject_binders: Vec<(Mutability, Var)>,
        tyargs: Vec<Type>,
        unpack: StructUnpack<(Fringe, PatternMatrix)>,
    },
    VariantSwitch {
        subject: FringeEntry,
        subject_binders: Vec<(Mutability, Var)>,
        tyargs: Vec<Type>,
        arms: BTreeMap<VariantName, (Vec<(Field, Var, Type)>, Fringe, PatternMatrix)>,
        default: (Fringe, PatternMatrix),
    },
}

#[derive(Clone)]
enum WorkResult {
    Leaf(Vec<ArmResult>),
    Failure,
    LiteralSwitch {
        subject: FringeEntry,
        subject_binders: Vec<(Mutability, Var)>,
        arms: BTreeMap<Value, usize>,
        default: usize, // default
    },
    StructUnpack {
        subject: FringeEntry,
        subject_binders: Vec<(Mutability, Var)>,
        tyargs: Vec<Type>,
        unpack: StructUnpack<usize>,
    },
    VariantSwitch {
        subject: FringeEntry,
        subject_binders: Vec<(Mutability, Var)>,
        tyargs: Vec<Type>,
        arms: BTreeMap<VariantName, (Vec<(Field, Var, Type)>, usize)>,
        default: usize,
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

    let mut compilation_results: BTreeMap<usize, WorkResult> = BTreeMap::new();

    let (mut initial_binders, init_subject, match_subject) = {
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
    };

    let mut work_queue: Vec<(usize, Fringe, PatternMatrix)> =
        vec![(0, VecDeque::from([match_subject]), pattern_matrix)];

    let mut work_id = 0;

    let mut next_id = || {
        work_id += 1;
        work_id
    };

    while let Some((cur_id, init_fringe, matrix)) = work_queue.pop() {
        debug_print!(
            context.debug.match_work_queue,
            ("work queue entry" => cur_id; fmt),
            (lines "fringe" => &init_fringe; sdbg),
            ("matrix" => matrix; verbose)
        );
        let redefined: Option<WorkResult> =
            match compile_match_head(context, init_fringe.clone(), matrix) {
                MatchStep::Leaf(leaf) => compilation_results.insert(cur_id, WorkResult::Leaf(leaf)),
                MatchStep::Failure => compilation_results.insert(cur_id, WorkResult::Failure),
                MatchStep::LiteralSwitch {
                    subject,
                    subject_binders,
                    fringe,
                    arms,
                    default,
                } => {
                    let mut answer_map = BTreeMap::new();
                    for (value, matrix) in arms {
                        let work_id = next_id();
                        answer_map.insert(value, work_id);
                        work_queue.push((work_id, fringe.clone(), matrix));
                    }
                    let default_work_id = next_id();
                    work_queue.push((default_work_id, fringe, default));
                    let result = WorkResult::LiteralSwitch {
                        subject,
                        subject_binders,
                        arms: answer_map,
                        default: default_work_id,
                    };
                    compilation_results.insert(cur_id, result)
                }
                MatchStep::StructUnpack {
                    subject,
                    subject_binders,
                    tyargs,
                    unpack,
                } => {
                    let unpack_work_id = next_id();
                    let unpack = match unpack {
                        StructUnpack::Default((fringe, matrix)) => {
                            work_queue.push((unpack_work_id, fringe, matrix));
                            StructUnpack::Default(unpack_work_id)
                        }
                        StructUnpack::Unpack(dtor_fields, (fringe, matrix)) => {
                            work_queue.push((unpack_work_id, fringe, matrix));
                            StructUnpack::Unpack(dtor_fields, unpack_work_id)
                        }
                    };
                    compilation_results.insert(
                        cur_id,
                        WorkResult::StructUnpack {
                            subject,
                            subject_binders,
                            tyargs,
                            unpack,
                        },
                    )
                }

                MatchStep::VariantSwitch {
                    subject,
                    subject_binders,
                    tyargs,
                    arms,
                    default: (dfringe, dmatrix),
                } => {
                    let mut answer_map = BTreeMap::new();
                    for (name, (dtor_fields, fringe, matrix)) in arms {
                        let work_id = next_id();
                        answer_map.insert(name, (dtor_fields, work_id));
                        work_queue.push((work_id, fringe, matrix));
                    }
                    let default_work_id = next_id();
                    work_queue.push((default_work_id, dfringe, dmatrix));
                    compilation_results.insert(
                        cur_id,
                        WorkResult::VariantSwitch {
                            subject,
                            subject_binders,
                            tyargs,
                            arms: answer_map,
                            default: default_work_id,
                        },
                    )
                }
            };
        ice_assert!(
            context.env,
            redefined.is_none(),
            loc,
            "Match work queue went awry"
        );
    }

    let match_start = compilation_results.remove(&0).unwrap();
    let mut resolution_context = ResolutionContext {
        hlir_context: context,
        output_type: result_type,
        arms: &arms,
        arms_loc: loc,
        results: &mut compilation_results,
    };
    let match_exp = resolve_result(&mut resolution_context, &init_subject, match_start);

    let eloc = match_exp.exp.loc;
    let mut seq = VecDeque::new();
    seq.append(&mut initial_binders);
    seq.push_back(sp(eloc, T::SequenceItem_::Seq(Box::new(match_exp))));
    let exp_value = sp(eloc, T::UnannotatedExp_::Block((UseFuns::new(0), seq)));
    T::exp(result_type.clone(), exp_value)
}

fn compile_match_head(
    context: &mut Context,
    mut fringe: VecDeque<FringeEntry>,
    mut matrix: PatternMatrix,
) -> MatchStep {
    debug_print!(
        context.debug.match_specialization,
        ("-----\ncompiling with fringe queue entry" => fringe; dbg)
    );
    if matrix.is_empty() {
        MatchStep::Failure
    } else if let Some(leaf) = matrix.wild_arm_opt(&fringe) {
        MatchStep::Leaf(leaf)
    } else if fringe[0].ty.value.unfold_to_builtin_type_name().is_some() {
        let subject = fringe
            .pop_front()
            .expect("ICE empty fringe in match compilation");
        let mut subject_binders = vec![];
        // treat column as a literal
        let lits = matrix.first_lits();
        let mut arms = BTreeMap::new();
        for lit in lits {
            let lit_loc = lit.loc;
            debug_print!(context.debug.match_specialization, ("lit specializing" => lit ; fmt));
            let (mut new_binders, inner_matrix) = matrix.specialize_literal(&lit);
            debug_print!(
                context.debug.match_specialization,
                ("binders" => &new_binders; dbg), ("specialized" => inner_matrix)
            );
            subject_binders.append(&mut new_binders);
            ice_assert!(
                context.env,
                arms.insert(lit, inner_matrix).is_none(),
                lit_loc,
                "Specialization failed"
            );
        }
        let (mut new_binders, default) = matrix.specialize_default();
        debug_print!(context.debug.match_specialization, ("default binders" => &new_binders; dbg));
        subject_binders.append(&mut new_binders);
        MatchStep::LiteralSwitch {
            subject,
            subject_binders,
            fringe,
            arms,
            default,
        }
    } else {
        let subject = fringe
            .pop_front()
            .expect("ICE empty fringe in match compilation");
        let tyargs = subject.ty.value.type_arguments().unwrap().clone();
        let mut subject_binders = vec![];
        debug_print!(
            context.debug.match_specialization,
            ("subject" => subject),
            ("matrix" => matrix)
        );
        let (mident, datatype_name) = subject
            .ty
            .value
            .unfold_to_type_name()
            .and_then(|sp!(_, name)| name.datatype_name())
            .expect("ICE non-datatype type in head constructor fringe position");

        if context.info.is_struct(&mident, &datatype_name) {
            // If we have an actual destructuring anywhere, we do that and take the specialized
            // matrix (which holds the default matrix and bindings, for our purpose). If we don't,
            // we just take the default matrix.
            let unpack = if let Some((ploc, arg_types)) = matrix.first_struct_ctors() {
                let fringe_binders = context.make_imm_ref_match_binders(ploc, arg_types);
                let fringe_exps = make_fringe_entries(&fringe_binders);
                let mut inner_fringe = fringe.clone();
                for fringe_exp in fringe_exps.into_iter().rev() {
                    inner_fringe.push_front(fringe_exp);
                }
                let bind_tys = fringe_binders
                    .iter()
                    .map(|(_, _, ty)| ty)
                    .collect::<Vec<_>>();
                debug_print!(
                    context.debug.match_specialization, ("struct specialized" => datatype_name; dbg)
                );
                let (mut new_binders, inner_matrix) = matrix.specialize_struct(context, bind_tys);
                debug_print!(context.debug.match_specialization,
                             ("binders" => new_binders; dbg),
                             ("specialized" => inner_matrix));
                subject_binders.append(&mut new_binders);
                StructUnpack::Unpack(fringe_binders, (inner_fringe, inner_matrix))
            } else {
                let (mut new_binders, default_matrix) = matrix.specialize_default();
                subject_binders.append(&mut new_binders);
                StructUnpack::Default((fringe, default_matrix))
            };
            MatchStep::StructUnpack {
                subject,
                subject_binders,
                tyargs,
                unpack,
            }
        } else {
            let mut unmatched_variants = context
                .info
                .enum_variants(&mident, &datatype_name)
                .into_iter()
                .collect::<BTreeSet<_>>();

            let ctors = matrix.first_variant_ctors();

            let mut arms = BTreeMap::new();
            for (ctor, (ploc, arg_types)) in ctors {
                unmatched_variants.remove(&ctor);
                let fringe_binders = context.make_imm_ref_match_binders(ploc, arg_types);
                let fringe_exps = make_fringe_entries(&fringe_binders);
                let mut inner_fringe = fringe.clone();
                for fringe_exp in fringe_exps.into_iter().rev() {
                    inner_fringe.push_front(fringe_exp);
                }
                let bind_tys = fringe_binders
                    .iter()
                    .map(|(_, _, ty)| ty)
                    .collect::<Vec<_>>();
                debug_print!(
                    context.debug.match_specialization, ("enum specialized" => datatype_name; dbg)
                );
                let (mut new_binders, inner_matrix) =
                    matrix.specialize_variant(context, &ctor, bind_tys);
                debug_print!(context.debug.match_specialization,
                             ("binders" => new_binders; dbg),
                             ("specialized" => inner_matrix));
                subject_binders.append(&mut new_binders);
                ice_assert!(
                    context.env,
                    arms.insert(ctor, (fringe_binders, inner_fringe, inner_matrix))
                        .is_none(),
                    ploc,
                    "Inserted duplicate ctor"
                );
            }

            let (mut new_binders, default_matrix) = matrix.specialize_default();
            subject_binders.append(&mut new_binders);

            MatchStep::VariantSwitch {
                subject,
                subject_binders,
                tyargs,
                arms,
                default: (fringe, default_matrix),
            }
        }
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
    results: &'call mut BTreeMap<usize, WorkResult>,
}

impl<'ctxt, 'call> ResolutionContext<'ctxt, 'call> {
    fn arm(&self, index: usize) -> T::Exp {
        self.arms[index].clone()
    }

    fn arms_loc(&self) -> Loc {
        self.arms_loc
    }

    fn work_result(&mut self, work_id: usize) -> WorkResult {
        self.results.remove(&work_id).unwrap()
    }

    fn copy_work_result(&mut self, work_id: usize) -> WorkResult {
        self.results.get(&work_id).unwrap().clone()
    }

    fn output_type(&self) -> Type {
        self.output_type.clone()
    }
}

#[growing_stack]
fn resolve_result(
    context: &mut ResolutionContext,
    init_subject: &FringeEntry,
    result: WorkResult,
) -> T::Exp {
    match result {
        WorkResult::Leaf(leaf) => make_leaf(context, init_subject, leaf),
        WorkResult::Failure => T::exp(
            context.output_type(),
            sp(context.arms_loc, T::UnannotatedExp_::UnresolvedError),
        ),
        WorkResult::VariantSwitch {
            subject,
            subject_binders,
            tyargs,
            mut arms,
            default: default_ndx,
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
                if let Some((unpack_fields, result_ndx)) = arms.remove(&v) {
                    let work_result = context.work_result(result_ndx);
                    let rest_result = resolve_result(context, init_subject, work_result);
                    let unpack_block = make_match_variant_unpack(
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
                    let work_result = context.copy_work_result(default_ndx);
                    let rest_result = resolve_result(context, init_subject, work_result);
                    blocks.push((v, rest_result));
                }
            }
            let out_exp = T::UnannotatedExp_::VariantMatch(make_var_ref(subject), (m, e), blocks);
            let body_exp = T::exp(context.output_type(), sp(context.arms_loc(), out_exp));
            make_copy_bindings(bindings, body_exp)
        }
        WorkResult::StructUnpack {
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
                StructUnpack::Default(result_ndx) => {
                    let work_result = context.work_result(result_ndx);
                    resolve_result(context, init_subject, work_result)
                }
                StructUnpack::Unpack(unpack_fields, result_ndx) => {
                    let work_result = context.work_result(result_ndx);
                    let rest_result = resolve_result(context, init_subject, work_result);
                    make_match_struct_unpack(
                        m,
                        s,
                        tyargs.clone(),
                        unpack_fields,
                        subject.clone(),
                        rest_result,
                    )
                }
            };
            make_copy_bindings(bindings, unpack_exp)
        }
        WorkResult::LiteralSwitch {
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
            let true_arm_ndx = arms
                .remove(&sp(Loc::invalid(), Value_::Bool(true)))
                .unwrap();
            let false_arm_ndx = arms
                .remove(&sp(Loc::invalid(), Value_::Bool(false)))
                .unwrap();

            let true_arm_result = context.work_result(true_arm_ndx);
            let false_arm_result = context.work_result(false_arm_ndx);

            let true_arm = resolve_result(context, init_subject, true_arm_result);
            let false_arm = resolve_result(context, init_subject, false_arm_result);
            let result_type = true_arm.ty.clone();

            make_copy_bindings(
                bindings,
                make_if_else(lit_subject, true_arm, false_arm, result_type),
            )
        }
        WorkResult::LiteralSwitch {
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

            let else_work_result = context.work_result(default);
            let mut out_exp = resolve_result(context, init_subject, else_work_result);

            for (key, result_ndx) in entries.into_iter().rev() {
                let work_result = context.work_result(result_ndx);
                let match_arm = resolve_result(context, init_subject, work_result);
                let test_exp = make_lit_test(lit_subject.clone(), key);
                let result_ty = out_exp.ty.clone();
                out_exp = make_if_else(test_exp, match_arm, out_exp, result_ty);
            }
            make_copy_bindings(bindings, out_exp)
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
            context.hlir_context.env,
            last.guard.is_none(),
            last.guard.unwrap().exp.loc,
            "Must have a non-guarded leaf"
        );
        return make_copy_bindings(last.bindings, make_arm(context, subject.clone(), last.arm));
    }

    let last = leaf.pop().unwrap();
    ice_assert!(
        context.hlir_context.env,
        last.guard.is_none(),
        last.guard.unwrap().exp.loc,
        "Must have a non-guarded leaf"
    );
    let mut out_exp =
        make_copy_bindings(last.bindings, make_arm(context, subject.clone(), last.arm));
    let out_ty = out_exp.ty.clone();
    while let Some(arm) = leaf.pop() {
        ice_assert!(
            context.hlir_context.env,
            arm.guard.is_some(),
            arm.loc,
            "Expected a guard"
        );
        out_exp = make_guard_exp(context, subject, arm, out_exp, out_ty.clone());
    }
    out_exp
}

fn make_guard_exp(
    context: &mut ResolutionContext,
    subject: &FringeEntry,
    arm: ArmResult,
    cur_exp: T::Exp,
    result_ty: Type,
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
    let body = make_if_else(*guard.unwrap(), guard_arm, cur_exp, result_ty);
    make_copy_bindings(bindings, body)
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
                    context.hlir_context.env.add_diag(ice!((
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
                    context.hlir_context.env.add_diag(ice!((
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
    let out_type = next.ty.clone();
    seq.push_back(sp(nloc, T::SequenceItem_::Seq(Box::new(next))));

    let body = T::UnannotatedExp_::Block((UseFuns::new(0), seq));
    T::exp(out_type, sp(ploc, body))
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
    let fringe_binders = context.hlir_context.make_unpack_binders(pat_loc, field_tys);
    let fringe_exps = make_fringe_entries(&fringe_binders);

    let decl_fields = context
        .hlir_context
        .info
        .enum_variant_fields(&mident, &enum_, &variant);
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
    let fringe_binders = context.hlir_context.make_unpack_binders(pat_loc, field_tys);
    let fringe_exps = make_fringe_entries(&fringe_binders);

    let decl_fields = context.hlir_context.info.struct_fields(&mident, &struct_);
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

    let result_type = next.ty.clone();
    let eloc = next.exp.loc;
    seq.push_back(sp(eloc, T::SequenceItem_::Seq(Box::new(next))));

    let exp_value = sp(eloc, T::UnannotatedExp_::Block((UseFuns::new(0), seq)));
    T::exp(result_type, exp_value)
}

// Performs a struct unpack for the purpose of matching, where we are matching against an imm. ref.
fn make_match_struct_unpack(
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

    let result_type = next.ty.clone();
    let eloc = next.exp.loc;
    seq.push_back(sp(eloc, T::SequenceItem_::Seq(Box::new(next))));

    let exp_value = sp(eloc, T::UnannotatedExp_::Block((UseFuns::new(0), seq)));
    T::exp(result_type, exp_value)
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

fn make_copy_bindings(bindings: PatBindings, next: T::Exp) -> T::Exp {
    make_bindings(bindings, next, true)
}

fn make_bindings(bindings: PatBindings, next: T::Exp, as_copy: bool) -> T::Exp {
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
    let result_type = next.ty.clone();
    seq.push_back(sp(eloc, T::SequenceItem_::Seq(Box::new(next))));
    let exp_value = sp(eloc, T::UnannotatedExp_::Block((UseFuns::new(0), seq)));
    T::exp(result_type, exp_value)
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

fn make_if_else(test: T::Exp, conseq: T::Exp, alt: T::Exp, result_ty: Type) -> T::Exp {
    // FIXME: this span is woefully wrong
    let loc = test.exp.loc;
    T::exp(
        result_ty,
        sp(
            loc,
            T::UnannotatedExp_::IfElse(Box::new(test), Box::new(conseq), Box::new(alt)),
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
