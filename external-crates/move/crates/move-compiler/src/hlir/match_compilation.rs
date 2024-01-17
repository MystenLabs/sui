// Copyright (c) The Move Contributors
// SPDX-License-Identifier: Apache-2.0

use crate::{
    diag,
    diagnostics::or_list_string,
    // diag,
    expansion::ast::{Fields, ModuleIdent, Value, Value_},
    hlir::translate::Context,
    naming::ast::{self as N, BuiltinTypeName_, Type, Var},
    parser::ast::{BinOp_, DatatypeName, Field, VariantName},
    shared::{
        ast_debug::{AstDebug, AstWriter},
        unique_map::UniqueMap,
    },
    typing::ast::{self as T, MatchArm_, MatchPattern, UnannotatedPat_ as TP},
};
use move_ir_types::location::*;
use std::{
    collections::{BTreeMap, BTreeSet, VecDeque},
    fmt::Display,
};

//**************************************************************************************************
// Description
//**************************************************************************************************
// This mostly follows the classical Maranget (2008) implementation toward optimal decision trees.

//**************************************************************************************************
// Match Trees
//**************************************************************************************************

#[derive(Clone, Debug)]
struct FringeEntry {
    var: Var,
    ty: Type,
}

type PatBindings = BTreeMap<Var, FringeEntry>;
type Guard = Option<Box<T::Exp>>;

#[derive(Clone, Debug)]
struct Arm {
    orig_pattern: MatchPattern,
    rhs_binders: BTreeSet<Var>,
    index: usize,
}

#[derive(Clone, Debug)]
struct PatternArm {
    pat: VecDeque<T::MatchPattern>,
    guard: Guard,
    arm: Arm,
}

#[derive(Clone, Debug)]
struct PatternMatrix {
    tys: Vec<Type>,
    patterns: Vec<PatternArm>,
}

#[derive(Clone, Debug)]
struct ArmResult {
    bindings: PatBindings,
    guard: Option<Box<T::Exp>>,
    arm: Arm,
}

impl FringeEntry {
    fn into_move_exp(self) -> T::Exp {
        let FringeEntry { var, ty } = self;
        let move_exp = T::UnannotatedExp_::Move {
            from_user: false,
            var,
        };
        T::exp(ty, sp(var.loc, move_exp))
    }
}

impl PatternArm {
    fn pattern_empty(&self) -> bool {
        self.pat.is_empty()
    }

    fn all_wild_arm(&mut self, fringe: &VecDeque<FringeEntry>) -> Option<ArmResult> {
        if self
            .pat
            .iter()
            .all(|pat| matches!(pat.pat.value, TP::Wildcard | TP::Binder(_)))
        {
            let bindings = self.make_arm_bindings(fringe);
            let PatternArm { pat: _, guard, arm } = self;
            let arm = ArmResult {
                bindings,
                guard: guard.clone(),
                arm: arm.clone(),
            };
            Some(arm)
        } else {
            None
        }
    }

    fn make_arm_bindings(&mut self, fringe: &VecDeque<FringeEntry>) -> PatBindings {
        let mut bindings = BTreeMap::new();
        for (pmut, subject) in self.pat.iter_mut().zip(fringe.iter()) {
            if let TP::Binder(x) = pmut.pat.value {
                if bindings.insert(x, subject.clone()).is_some() {
                    panic!("ICE should have failed in naming");
                };
                pmut.pat.value = TP::Wildcard;
            }
        }
        bindings
    }

    fn first_ctor(&self) -> BTreeMap<VariantName, (Loc, Fields<Type>)> {
        if self.pat.is_empty() {
            return BTreeMap::new();
        }
        let mut names = BTreeMap::new();
        let mut ctor_queue = vec![self.pat.front().unwrap().clone()];
        while let Some(pat) = ctor_queue.pop() {
            match pat.pat.value {
                TP::Constructor(_, _, name, _, fields) => {
                    let ty_fields: Fields<Type> = fields.clone().map(|_, (ndx, (ty, _))| (ndx, ty));
                    names.insert(name, (pat.pat.loc, ty_fields));
                }
                TP::BorrowConstructor(_, _, name, _, fields) => {
                    let ty_fields: Fields<Type> = fields.clone().map(|_, (ndx, (ty, _))| (ndx, ty));
                    names.insert(name, (pat.pat.loc, ty_fields));
                }
                TP::Binder(_) => (),
                TP::Literal(_) => (),
                TP::Wildcard => (),
                TP::Or(lhs, rhs) => {
                    ctor_queue.push(*lhs);
                    ctor_queue.push(*rhs);
                }
                TP::At(_, inner) => {
                    ctor_queue.push(*inner);
                }
                TP::ErrorPat => (),
            }
        }
        names
    }

    fn first_lit(&self) -> BTreeSet<Value> {
        if self.pat.is_empty() {
            return BTreeSet::new();
        }
        let mut values = BTreeSet::new();
        let mut ctor_queue = vec![self.pat.front().unwrap().clone()];
        while let Some(pat) = ctor_queue.pop() {
            match pat.pat.value {
                TP::Constructor(_, _, _, _, _) => (),
                TP::BorrowConstructor(_, _, _, _, _) => (),
                TP::Binder(_) => (),
                TP::Literal(v) => {
                    values.insert(v);
                }
                TP::Wildcard => (),
                TP::Or(lhs, rhs) => {
                    ctor_queue.push(*lhs);
                    ctor_queue.push(*rhs);
                }
                TP::At(_, inner) => {
                    ctor_queue.push(*inner);
                }
                TP::ErrorPat => (),
            }
        }
        values
    }

    fn specialize(
        &self,
        context: &Context,
        ctor_name: &VariantName,
        arg_types: &Vec<&Type>,
    ) -> Option<(Vec<Var>, PatternArm)> {
        let mut output = self.clone();
        let first_pattern = output.pat.pop_front().unwrap();
        let loc = first_pattern.pat.loc;
        match first_pattern.pat.value {
            TP::Constructor(mident, enum_, name, _, fields)
            | TP::BorrowConstructor(mident, enum_, name, _, fields)
                if &name == ctor_name =>
            {
                let field_pats = fields.clone().map(|_key, (ndx, (_, pat))| (ndx, pat));
                let decl_fields = context.enum_variant_fields(&mident, &enum_, &name);
                let ordered_pats = order_fields_by_decl(decl_fields, field_pats);
                for (_, _, pat) in ordered_pats.into_iter().rev() {
                    output.pat.push_front(pat);
                }
                Some((vec![], output))
            }
            TP::Constructor(_, _, _, _, _) | TP::BorrowConstructor(_, _, _, _, _) => None,
            TP::Literal(_) => None,
            TP::Binder(x) => {
                for arg_type in arg_types
                    .clone()
                    .into_iter()
                    .map(|ty| ty_to_wildcard_pattern(ty.clone(), loc))
                    .rev()
                {
                    output.pat.push_front(arg_type);
                }
                Some((vec![x], output))
            }
            TP::Wildcard => {
                for arg_type in arg_types
                    .clone()
                    .into_iter()
                    .map(|ty| ty_to_wildcard_pattern(ty.clone(), loc))
                    .rev()
                {
                    output.pat.push_front(arg_type);
                }
                Some((vec![], output))
            }
            TP::Or(_, _) => unreachable!(),
            TP::At(x, inner) => {
                output.pat.push_front(*inner);
                let inner_spec = output.specialize(context, ctor_name, arg_types);
                match inner_spec {
                    None => None,
                    Some((mut v, inner)) => {
                        v.push(x);
                        Some((v, inner))
                    }
                }
            }
            TP::ErrorPat => None,
        }
    }

    fn specialize_literal(&self, literal: &Value) -> Option<(Vec<Var>, PatternArm)> {
        let mut output = self.clone();
        let first_pattern = output.pat.pop_front().unwrap();
        match first_pattern.pat.value {
            TP::Literal(v) if &v == literal => Some((vec![], output)),
            TP::Literal(_) => None,
            TP::Constructor(_, _, _, _, _) | TP::BorrowConstructor(_, _, _, _, _) => None,
            TP::Binder(x) => Some((vec![x], output)),
            TP::Wildcard => Some((vec![], output)),
            TP::Or(_, _) => unreachable!(),
            TP::At(x, inner) => {
                output.pat.push_front(*inner);
                let inner_spec = output.specialize_literal(literal);
                match inner_spec {
                    None => None,
                    Some((mut v, inner)) => {
                        v.push(x);
                        Some((v, inner))
                    }
                }
            }
            TP::ErrorPat => None,
        }
    }

    fn default(&self) -> Option<(Vec<Var>, PatternArm)> {
        let mut output = self.clone();
        let first_pattern = output.pat.pop_front().unwrap();
        match first_pattern.pat.value {
            TP::Literal(_) => None,
            TP::Constructor(_, _, _, _, _) | TP::BorrowConstructor(_, _, _, _, _) => None,
            TP::Binder(x) => Some((vec![x], output)),
            TP::Wildcard => Some((vec![], output)),
            TP::Or(_, _) => unreachable!(),
            TP::At(x, inner) => {
                output.pat.push_front(*inner);
                let inner_spec = output.default();
                match inner_spec {
                    None => None,
                    Some((mut v, inner)) => {
                        v.push(x);
                        Some((v, inner))
                    }
                }
            }
            TP::ErrorPat => None,
        }
    }
}

impl PatternMatrix {
    fn from(subject_ty: Type, arms: Vec<T::MatchArm>) -> (PatternMatrix, Vec<T::Exp>) {
        fn apply_pattern_subst(pat: MatchPattern, env: &UniqueMap<Var, Var>) -> MatchPattern {
            let MatchPattern {
                ty,
                pat: sp!(ploc, pat),
            } = pat;
            let new_pat = match pat {
                TP::Constructor(m, e, v, ta, spats) => {
                    let out_fields =
                        spats.map(|_, (ndx, (t, pat))| (ndx, (t, apply_pattern_subst(pat, env))));
                    TP::Constructor(m, e, v, ta, out_fields)
                }
                TP::BorrowConstructor(m, e, v, ta, spats) => {
                    let out_fields =
                        spats.map(|_, (ndx, (t, pat))| (ndx, (t, apply_pattern_subst(pat, env))));
                    TP::Constructor(m, e, v, ta, out_fields)
                }
                TP::At(x, inner) => {
                    let xloc = x.loc;
                    if let Some(y) = env.get(&x) {
                        TP::At(
                            sp(xloc, y.value),
                            Box::new(apply_pattern_subst(*inner, env)),
                        )
                    } else {
                        apply_pattern_subst(*inner, env).pat.value
                    }
                }
                TP::Binder(x) => {
                    let xloc = x.loc;
                    if let Some(y) = env.get(&x) {
                        TP::Binder(sp(xloc, y.value))
                    } else {
                        TP::Wildcard
                    }
                }
                pat @ (TP::Literal(_) | TP::ErrorPat | TP::Wildcard) => pat,
                TP::Or(_, _) => unreachable!(),
            };
            MatchPattern {
                ty,
                pat: sp(ploc, new_pat),
            }
        }

        let tys = vec![subject_ty];
        let mut patterns = vec![];
        let mut rhss = vec![];
        for sp!(_, arm) in arms {
            let MatchArm_ {
                pattern,
                binders: _,
                guard,
                guard_binders,
                rhs_binders,
                rhs,
            } = arm;
            rhss.push(*rhs);
            let index = rhss.len() - 1;
            let new_patterns = flatten_or(pattern);
            for pat in new_patterns {
                let arm = Arm {
                    orig_pattern: pat.clone(),
                    rhs_binders: rhs_binders.clone(),
                    index,
                };
                let guard = guard.clone();
                // Make a match pattern that only holds guard binders
                let match_pattern = apply_pattern_subst(pat, &guard_binders);
                patterns.push(PatternArm {
                    pat: VecDeque::from([match_pattern]),
                    guard,
                    arm,
                });
            }
        }
        (PatternMatrix { tys, patterns }, rhss)
    }

    fn is_empty(&self) -> bool {
        self.patterns.is_empty()
    }

    fn patterns_empty(&self) -> bool {
        !self.patterns.is_empty() && self.patterns.iter().all(|pat| pat.pattern_empty())
    }

    fn wild_arm_opt(&mut self, fringe: &VecDeque<FringeEntry>) -> Option<Vec<ArmResult>> {
        // NB: If the first row is all wild, we need to collect _all_ wild rows that have guards
        // until we find one that does not.
        if let Some(arm) = self.patterns[0].all_wild_arm(fringe) {
            if arm.guard.is_none() {
                return Some(vec![arm]);
            }
            let mut result = vec![arm];
            for pat in self.patterns[1..].iter_mut() {
                if let Some(arm) = pat.all_wild_arm(fringe) {
                    let has_guard = arm.guard.is_some();
                    result.push(arm);
                    if !has_guard {
                        return Some(result);
                    }
                }
            }
            Some(result)
        } else {
            None
        }
    }

    fn specialize(
        &self,
        context: &Context,
        ctor_name: &VariantName,
        arg_types: Vec<&Type>,
    ) -> (Vec<Var>, PatternMatrix) {
        let mut patterns = vec![];
        let mut bindings = vec![];
        for entry in &self.patterns {
            if let Some((mut new_bindings, arm)) = entry.specialize(context, ctor_name, &arg_types)
            {
                bindings.append(&mut new_bindings);
                patterns.push(arm)
            }
        }
        let mut tys = arg_types.into_iter().cloned().collect::<Vec<_>>();
        let mut old_tys = self.tys.clone();
        old_tys.remove(0);
        tys.extend(&mut old_tys.into_iter());
        let matrix = PatternMatrix { tys, patterns };
        (bindings, matrix)
    }

    fn specialize_literal(&self, lit: &Value) -> (Vec<Var>, PatternMatrix) {
        let mut patterns = vec![];
        let mut bindings = vec![];
        for entry in &self.patterns {
            if let Some((mut new_bindings, arm)) = entry.specialize_literal(lit) {
                bindings.append(&mut new_bindings);
                patterns.push(arm)
            }
        }
        let mut tys = self.tys.clone();
        tys.remove(0);
        let matrix = PatternMatrix { tys, patterns };
        (bindings, matrix)
    }

    fn default(&self) -> (Vec<Var>, PatternMatrix) {
        let mut patterns = vec![];
        let mut bindings = vec![];
        for entry in &self.patterns {
            if let Some((mut new_bindings, arm)) = entry.default() {
                bindings.append(&mut new_bindings);
                patterns.push(arm)
            }
        }
        let mut tys = self.tys.clone();
        tys.remove(0);
        let matrix = PatternMatrix { tys, patterns };
        (bindings, matrix)
    }

    fn first_head_ctors(&self) -> BTreeMap<VariantName, (Loc, Fields<Type>)> {
        let mut ctors = BTreeMap::new();
        for pat in &self.patterns {
            ctors.append(&mut pat.first_ctor());
        }
        ctors
    }

    fn first_lits(&self) -> BTreeSet<Value> {
        let mut ctors = BTreeSet::new();
        for pat in &self.patterns {
            ctors.append(&mut pat.first_lit());
        }
        ctors
    }

    fn has_guards(&self) -> bool {
        self.patterns.iter().any(|pat| pat.guard.is_some())
    }

    fn remove_guards(&mut self) {
        let pats = std::mem::take(&mut self.patterns);
        self.patterns = pats.into_iter().filter(|pat| pat.guard.is_none()).collect();
    }
}

fn ty_to_wildcard_pattern(ty: Type, loc: Loc) -> T::MatchPattern {
    T::MatchPattern {
        ty,
        pat: sp(loc, T::UnannotatedPat_::Wildcard),
    }
}

fn flatten_or(pat: MatchPattern) -> Vec<MatchPattern> {
    if matches!(
        pat.pat.value,
        TP::Literal(_) | TP::Binder(_) | TP::Wildcard | TP::ErrorPat
    ) {
        vec![pat]
    } else if matches!(
    &pat.pat.value,
    TP::Constructor(_, _, _, _, pats) | TP::BorrowConstructor(_, _, _, _, pats)
        if pats.is_empty()
    ) {
        return vec![pat];
    } else {
        let MatchPattern {
            ty,
            pat: sp!(ploc, pat_),
        } = pat;
        match pat_ {
            TP::Constructor(m, e, v, ta, spats) => {
                let all_spats = spats.map(|_, (ndx, (t, pat))| (ndx, (t, flatten_or(pat))));
                let fields_lists: Vec<Fields<(Type, MatchPattern)>> =
                    combine_pattern_fields(all_spats);
                fields_lists
                    .into_iter()
                    .map(|field_list| MatchPattern {
                        ty: ty.clone(),
                        pat: sp(ploc, TP::Constructor(m, e, v, ta.clone(), field_list)),
                    })
                    .collect::<Vec<_>>()
            }
            TP::BorrowConstructor(m, e, v, ta, spats) => {
                let all_spats = spats.map(|_, (ndx, (t, pat))| (ndx, (t, flatten_or(pat))));
                let fields_lists: Vec<Fields<(Type, MatchPattern)>> =
                    combine_pattern_fields(all_spats);
                fields_lists
                    .into_iter()
                    .map(|field_list| MatchPattern {
                        ty: ty.clone(),
                        pat: sp(ploc, TP::BorrowConstructor(m, e, v, ta.clone(), field_list)),
                    })
                    .collect::<Vec<_>>()
            }
            TP::Or(lhs, rhs) => {
                let mut lhs_rec = flatten_or(*lhs);
                let mut rhs_rec = flatten_or(*rhs);
                lhs_rec.append(&mut rhs_rec);
                lhs_rec
            }
            TP::At(x, inner) => flatten_or(*inner)
                .into_iter()
                .map(|pat| MatchPattern {
                    ty: ty.clone(),
                    pat: sp(ploc, TP::At(x, Box::new(pat))),
                })
                .collect::<Vec<_>>(),
            TP::Literal(_) | TP::Binder(_) | TP::Wildcard | TP::ErrorPat => unreachable!(),
        }
    }
}

fn combine_pattern_fields(
    fields: Fields<(Type, Vec<MatchPattern>)>,
) -> Vec<Fields<(Type, MatchPattern)>> {
    type VFields = Vec<(Field, (usize, (Spanned<N::Type_>, MatchPattern)))>;
    type VVFields = Vec<(Field, (usize, (Spanned<N::Type_>, Vec<MatchPattern>)))>;

    fn combine_recur(vec: &mut VVFields) -> Vec<VFields> {
        if let Some((f, (ndx, (ty, pats)))) = vec.pop() {
            let rec_fields = combine_recur(vec);
            // println!("rec fields: {:?}", rec_fields);
            let mut output = vec![];
            for entry in rec_fields {
                for pat in pats.clone() {
                    let mut entry = entry.clone();
                    entry.push((f, (ndx, (ty.clone(), pat))));
                    output.push(entry);
                }
            }
            // println!("output: {:?}", output);
            output
        } else {
            // Base case: a single match of no fields. We must have at least one, or else we would
            // not have called `combine_match_patterns`.
            vec![vec![]]
        }
    }

    fn vfields_to_fields(vfields: VFields) -> Fields<(Type, MatchPattern)> {
        UniqueMap::maybe_from_iter(vfields.into_iter()).unwrap()
    }

    // println!("init fields: {:?}", fields);
    let mut vvfields: VVFields = fields.into_iter().collect::<Vec<_>>();
    // println!("vv fields: {:?}", vvfields);
    let output_vec = combine_recur(&mut vvfields);
    // println!("output: {:?}", output_vec);
    output_vec
        .into_iter()
        .map(vfields_to_fields)
        .collect::<Vec<_>>()
}

//**************************************************************************************************
// Match Compilation
//**************************************************************************************************

type Fringe = VecDeque<FringeEntry>;

enum MatchStep {
    Leaf(Vec<ArmResult>),
    Failure,
    LiteralSwitch {
        subject: FringeEntry,
        subject_binders: Vec<Var>,
        fringe: Fringe,
        arms: BTreeMap<Value, PatternMatrix>,
        default: PatternMatrix,
    },
    VariantSwitch {
        subject: FringeEntry,
        subject_binders: Vec<Var>,
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
        subject_binders: Vec<Var>,
        arms: BTreeMap<Value, usize>,
        default: usize, // default
    },
    VariantSwitch {
        subject: FringeEntry,
        subject_binders: Vec<Var>,
        tyargs: Vec<Type>,
        arms: BTreeMap<VariantName, (Vec<(Field, Var, Type)>, usize)>,
        default: usize,
    },
}

pub fn compile_match(
    context: &mut Context,
    result_type: &Type,
    subject: T::Exp,
    arms: Spanned<Vec<T::MatchArm>>,
) -> T::Exp {
    let arms_loc = arms.loc;
    let (pattern_matrix, arms) = PatternMatrix::from(subject.ty.clone(), arms.value);

    let mut counterexample_matrix = pattern_matrix.clone();
    let has_guards = counterexample_matrix.has_guards();
    counterexample_matrix.remove_guards();
    if find_counterexample(context, subject.exp.loc, counterexample_matrix, has_guards) {
        return T::exp(
            result_type.clone(),
            sp(subject.exp.loc, T::UnannotatedExp_::UnresolvedError),
        );
    }

    let mut compilation_results: BTreeMap<usize, WorkResult> = BTreeMap::new();

    let (mut initial_binders, init_subject, match_subject) = {
        let subject_var = context.new_match_var("match_subject".to_string(), arms_loc);
        let subject_loc = subject.exp.loc;
        let match_var = context.new_match_var("match_subject".to_string(), arms_loc);

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
            let lhs_lvalue = make_lvalue(subject_var, subject.ty.clone());
            let binder = T::SequenceItem_::Bind(
                sp(lhs_loc, vec![lhs_lvalue]),
                vec![Some(subject.ty.clone())],
                Box::new(subject),
            );
            sp(lhs_loc, binder)
        };

        let subject_borrow = {
            let lhs_loc = arms_loc;
            let lhs_lvalue = make_lvalue(match_var, subject_borrow_rhs.ty.clone());
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
        // println!("---\nwork queue entry: {}", cur_id);
        // println!("fringe:");
        // for elem in &init_fringe {
        //     print!("  ");
        //     elem.print_verbose();
        // }
        // println!("matrix:");
        // matrix.print_verbose();
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
        assert!(redefined.is_none(), "ICE match work queue went awry");
    }

    let match_start = compilation_results.remove(&0).unwrap();
    let mut resolution_context = ResolutionContext {
        hlir_context: context,
        output_type: result_type,
        arms: &arms,
        arms_loc,
        results: &mut compilation_results,
    };
    let match_exp = resolve_result(&mut resolution_context, &init_subject, match_start);

    let eloc = match_exp.exp.loc;
    let mut seq = VecDeque::new();
    seq.append(&mut initial_binders);
    seq.push_back(sp(eloc, T::SequenceItem_::Seq(Box::new(match_exp))));
    let exp_value = sp(eloc, T::UnannotatedExp_::Block(seq));
    T::exp(result_type.clone(), exp_value)
}

fn compile_match_head(
    context: &mut Context,
    mut fringe: VecDeque<FringeEntry>,
    mut matrix: PatternMatrix,
) -> MatchStep {
    // println!("------\ncompilning with fringe:");
    // println!("{:#?}", fringe);
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
            // println!("specializing to {:?}", lit);
            let (mut new_binders, inner_matrix) = matrix.specialize_literal(&lit);
            // println!("binders: {:#?}", new_binders);
            subject_binders.append(&mut new_binders);
            // println!("specialized:");
            // inner_matrix.print();
            assert!(arms.insert(lit, inner_matrix).is_none());
        }
        let (mut new_binders, default) = matrix.default();
        // println!("default binders: {:#?}", new_binders);
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
        let mut subject_binders = vec![];
        // println!("------\nsubject:");
        // subject.print();
        // println!("--\ncompile match head:");
        // subject.print();
        // println!("--\nmatrix;");
        // matrix.print();

        let (mident, datatype_name) = subject
            .ty
            .value
            .unfold_to_type_name()
            .and_then(|sp!(_, name)| name.datatype_name())
            .expect("ICE non-datatype type in head constructor fringe position");

        // TODO: enable this later.
        if context.is_struct(&mident, &datatype_name) {
            panic!("ICE should have been handled by the leaf case or failed in naming / typing");
        }

        let tyargs = subject.ty.value.type_arguments().unwrap().clone();
        // treat it as a head constructor
        // assert!(!ctors.is_empty());

        let mut unmatched_variants = context
            .enum_variants(&mident, &datatype_name)
            .into_iter()
            .collect::<BTreeSet<_>>();

        let ctors = matrix.first_head_ctors();

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
            // println!("specializing to {:?}", ctor);
            let (mut new_binders, inner_matrix) = matrix.specialize(context, &ctor, bind_tys);
            // println!("binders: {:#?}", new_binders);
            subject_binders.append(&mut new_binders);
            // println!("specialized:");
            // inner_matrix.print();
            assert!(arms
                .insert(ctor, (fringe_binders, inner_fringe, inner_matrix))
                .is_none());
        }

        let (mut new_binders, default_matrix) = matrix.default();
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

pub fn order_fields_by_decl<T>(
    decl_fields: Option<&UniqueMap<Field, usize>>,
    fields: Fields<T>,
) -> Vec<(usize, Field, T)> {
    let mut texp_fields: Vec<(usize, Field, T)> = if let Some(field_map) = decl_fields {
        fields
            .into_iter()
            .map(|(f, (_exp_idx, t))| (*field_map.get(&f).unwrap(), f, t))
            .collect()
    } else {
        // If no field map, compiler error in typing.
        fields
            .into_iter()
            .enumerate()
            .map(|(ndx, (f, (_exp_idx, t)))| (ndx, f, t))
            .collect()
    };
    texp_fields.sort_by(|(decl_idx1, _, _), (decl_idx2, _, _)| decl_idx1.cmp(decl_idx2));
    texp_fields
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
            let bindings = subject_binders
                .into_iter()
                .map(|binder| (binder, subject.clone()))
                .collect();

            let sorted_variants: Vec<VariantName> = context.hlir_context.enum_variants(&m, &e);
            let mut blocks = vec![];
            for v in sorted_variants {
                if let Some((unpack_fields, result_ndx)) = arms.remove(&v) {
                    let work_result = context.work_result(result_ndx);
                    let rest_result = resolve_result(context, init_subject, work_result);
                    let unpack_block = make_match_unpack(
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
            let out_exp = T::UnannotatedExp_::VariantMatch(make_var_ref(subject), e, blocks);
            let body_exp = T::exp(context.output_type(), sp(context.arms_loc(), out_exp));
            make_copy_bindings(bindings, body_exp)
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
            let bindings = subject_binders
                .into_iter()
                .map(|binder| (binder, subject.clone()))
                .collect();
            // If the literal switch for a boolean is saturated, no default case.
            let lit_subject = make_lit_copy(subject.clone());
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
            let bindings = subject_binders
                .into_iter()
                .map(|binder| (binder, subject.clone()))
                .collect();
            let lit_subject = make_lit_copy(subject.clone());

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
        if last.guard.is_some() {
            panic!("ICE must have a non-guarded leaf, got {:#?}", last);
        }
        return make_copy_bindings(last.bindings, make_arm(context, subject.clone(), last.arm));
    }

    let last = leaf.pop().unwrap();
    assert!(last.guard.is_none(), "ICE must have a non-guarded leaf");
    let mut out_exp =
        make_copy_bindings(last.bindings, make_arm(context, subject.clone(), last.arm));
    let out_ty = out_exp.ty.clone();
    while let Some(arm) = leaf.pop() {
        assert!(arm.guard.is_some(), "ICE expected a guard");
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
        bindings,
        guard,
        arm,
    } = arm;
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
    // TODO(cgswords): optimization opportunity to avoid unpacking if the pattern is all wildcards.
    // It raises the question, though: should we _drop_ that value? If there is an at-binding,
    // maybe we don't want that, e.g., `x @ Some(_) => ... x ...` may like to avoid the unpack and
    // retain `x`. Rust does something similar.
    while let Some((entry, pat)) = queue.pop_front() {
        match pat.pat.value {
            TP::Constructor(mident, enum_, variant, tyargs, fields)
            | TP::BorrowConstructor(mident, enum_, variant, tyargs, fields) => {
                let all_wild = fields
                         .iter()
                         .all(|(_, _, (_, (_, pat)))| matches!(pat.pat.value, TP::Wildcard)) || fields.is_empty();
                if matches!(entry.ty.value, N::Type_::Ref(_, _)) && all_wild {
                     continue;
                 }
                let field_pats = fields.clone().map(|_key, (ndx, (_, pat))| (ndx, pat));

                let field_tys = fields.map(|_key, (ndx, (ty, _))| (ndx, ty));
                let fringe_binders = context
                    .hlir_context
                    .make_unpack_binders(pat.pat.loc, field_tys);
                let fringe_exps = make_fringe_entries(&fringe_binders);

                let decl_fields = context
                    .hlir_context
                    .enum_variant_fields(&mident, &enum_, &variant);
                let ordered_pats = order_fields_by_decl(decl_fields, field_pats);

                let mut unpack_fields: Vec<(Field, Var, Type)> = vec![];
                for (fringe_exp, (_, field, _)) in fringe_exps.iter().zip(ordered_pats.iter()) {
                    unpack_fields.push((*field, fringe_exp.var, fringe_exp.ty.clone()));
                }
                for (fringe_exp, (_, _, ordered_pat)) in
                    fringe_exps.into_iter().zip(ordered_pats.into_iter()).rev()
                {
                    queue.push_front((fringe_exp, ordered_pat));
                }
                let unpack =
                    make_unpack_stmt(mident, enum_, variant, tyargs, unpack_fields, entry, false);
                seq.push_back(unpack);
            }
            TP::Literal(_) => (),
            TP::Binder(x) if rhs_binders.contains(&x) => {
                seq.push_back(make_move_binding(x, entry.ty.clone(), entry))
            }
            TP::Binder(_) => (),
            TP::Wildcard => (),
            TP::Or(_, _) => unreachable!(),
            TP::At(x, inner) => {
                if rhs_binders.contains(&x) {
                    let bind_entry = entry.clone();
                    seq.push_back(make_move_binding(x, bind_entry.ty.clone(), bind_entry));
                }
                queue.push_front((entry, *inner));
            }
            TP::ErrorPat => (),
        }
    }

    let nloc = next.exp.loc;
    let out_type = next.ty.clone();
    seq.push_back(sp(nloc, T::SequenceItem_::Seq(Box::new(next))));

    let body = T::UnannotatedExp_::Block(seq);
    T::exp(out_type, sp(ploc, body))
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
            let loc = var.loc;

            // NB(cswords): we now freeze the mut ref at the non-mut ref type.

            let ref_ty = sp(loc, N::Type_::Ref(true, inner.clone()));
            let freeze_arg = make_copy_exp(ref_ty, loc, var);
            let freeze_ty = sp(loc, N::Type_::Ref(false, inner));
            Box::new(make_freeze_exp(freeze_ty, loc, freeze_arg))
        }
        ty => {
            let loc = var.loc;
            let ref_ty = sp(loc, N::Type_::Ref(false, Box::new(ty)));
            let borrow_exp = T::UnannotatedExp_::BorrowLocal(false, var);
            Box::new(T::exp(ref_ty, sp(loc, borrow_exp)))
        }
    }
}

fn make_match_unpack(
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
    seq.push_back(make_unpack_stmt(
        mident, enum_, variant, tyargs, fields, rhs, true,
    ));
    let result_type = next.ty.clone();
    let eloc = next.exp.loc;
    seq.push_back(sp(eloc, T::SequenceItem_::Seq(Box::new(next))));
    let exp_value = sp(eloc, T::UnannotatedExp_::Block(seq));
    T::exp(result_type, exp_value)
}

fn make_unpack_stmt(
    mident: ModuleIdent,
    enum_: DatatypeName,
    variant: VariantName,
    tyargs: Vec<Type>,
    fields: Vec<(Field, Var, Type)>,
    rhs: FringeEntry,
    rhs_as_var_ref: bool,
) -> T::SequenceItem {
    let rhs_loc = rhs.var.loc;
    let mut lvalue_fields: Fields<(Type, T::LValue)> = UniqueMap::new();

    for (ndx, (field_name, var, ty)) in fields.into_iter().enumerate() {
        let var_lvalue = make_lvalue(var, ty.clone());
        lvalue_fields
            .add(field_name, (ndx, (ty, var_lvalue)))
            .unwrap();
    }

    let unpack_lvalue = match rhs.ty.value {
        N::Type_::Ref(mut_, _) => sp(
            rhs_loc,
            T::LValue_::BorrowUnpackVariant(mut_, mident, enum_, variant, tyargs, lvalue_fields),
        ),
        _ => sp(
            rhs_loc,
            T::LValue_::UnpackVariant(mident, enum_, variant, tyargs, lvalue_fields),
        ),
    };
    let rhs_ty = rhs.ty.clone();
    let rhs: Box<T::Exp> = if rhs_as_var_ref {
        make_unpack_var_ref(rhs)
    } else {
        Box::new(rhs.into_move_exp())
    };
    let binder = T::SequenceItem_::Bind(sp(rhs_loc, vec![unpack_lvalue]), vec![Some(rhs_ty)], rhs);
    sp(rhs_loc, binder)
}

fn make_unpack_var_ref(subject: FringeEntry) -> Box<T::Exp> {
    let FringeEntry { var, ty } = subject;
    match ty {
        sp!(_, N::Type_::Ref(false, _)) => Box::new(make_copy_exp(ty, var.loc, var)),
        sp!(_, N::Type_::Ref(true, inner)) => {
            let loc = var.loc;

            // NB(cswords): we now freeze the mut ref at the non-mut ref type.

            let ref_ty = sp(loc, N::Type_::Ref(true, inner.clone()));
            let freeze_arg = make_copy_exp(ref_ty, loc, var);
            let freeze_ty = sp(loc, N::Type_::Ref(false, inner));
            Box::new(make_freeze_exp(freeze_ty, loc, freeze_arg))
        }
        ty => {
            let loc = var.loc;
            let ref_ty = sp(loc, N::Type_::Ref(false, Box::new(ty)));
            let borrow_exp = T::UnannotatedExp_::BorrowLocal(false, var);
            Box::new(T::exp(ref_ty, sp(loc, borrow_exp)))
        }
    }
}

fn make_lit_copy(subject: FringeEntry) -> T::Exp {
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
        ty => {
            let loc = var.loc;
            make_copy_exp(ty, loc, var)
        }
    }
}

fn make_copy_bindings(bindings: PatBindings, next: T::Exp) -> T::Exp {
    make_bindings(bindings, next, true)
}

fn make_bindings(bindings: PatBindings, next: T::Exp, as_copy: bool) -> T::Exp {
    let eloc = next.exp.loc;
    let mut seq = VecDeque::new();
    for (lhs, rhs) in bindings {
        let binding = if as_copy {
            make_copy_binding(lhs, rhs.ty.clone(), rhs)
        } else {
            make_move_binding(lhs, rhs.ty.clone(), rhs)
        };
        seq.push_back(binding);
    }
    let result_type = next.ty.clone();
    seq.push_back(sp(eloc, T::SequenceItem_::Seq(Box::new(next))));
    let exp_value = sp(eloc, T::UnannotatedExp_::Block(seq));
    T::exp(result_type, exp_value)
}

fn make_lvalue(lhs: Var, ty: Type) -> T::LValue {
    let lhs_loc = lhs.loc;
    let lhs_var = T::LValue_::Var {
        var: lhs,
        ty: Box::new(ty.clone()),
        unused_binding: false,
    };
    sp(lhs_loc, lhs_var)
}

fn make_move_binding(lhs: Var, ty: Type, rhs: FringeEntry) -> T::SequenceItem {
    let lhs_loc = lhs.loc;
    let lhs_lvalue = make_lvalue(lhs, ty.clone());
    let binder = T::SequenceItem_::Bind(
        sp(lhs_loc, vec![lhs_lvalue]),
        vec![Some(ty)],
        Box::new(rhs.into_move_exp()),
    );
    sp(lhs_loc, binder)
}

fn make_copy_binding(lhs: Var, ty: Type, rhs: FringeEntry) -> T::SequenceItem {
    let lhs_loc = lhs.loc;
    let lhs_lvalue = make_lvalue(lhs, ty.clone());
    let binder = T::SequenceItem_::Bind(
        sp(lhs_loc, vec![lhs_lvalue]),
        vec![Some(ty.clone())],
        Box::new(make_copy_exp(ty, rhs.var.loc, rhs.var)),
    );
    sp(lhs_loc, binder)
}

fn make_lit_test(lit_exp: T::Exp, value: Value) -> T::Exp {
    let loc = value.loc;
    let value_exp = Box::new(T::exp(
        lit_exp.ty.clone(),
        sp(loc, T::UnannotatedExp_::Value(value)),
    ));
    let bool = N::Type_::bool(loc);
    let equality_exp_ = T::UnannotatedExp_::BinopExp(
        Box::new(lit_exp),
        sp(loc, BinOp_::Eq),
        Box::new(bool.clone()),
        value_exp,
    );
    T::exp(bool, sp(loc, equality_exp_))
}

fn make_if_else(test: T::Exp, conseq: T::Exp, alt: T::Exp, result_ty: Type) -> T::Exp {
    // FIXME: this span is woefully wrong
    let loc = conseq.exp.loc;
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

//------------------------------------------------
// Counterexample Generation
//------------------------------------------------

#[derive(Clone, Debug)]
enum CounterExample {
    Wildcard,
    Literal(String),
    Constructor(DatatypeName, VariantName, Vec<CounterExample>),
    Note(String, Box<CounterExample>),
}

impl CounterExample {
    fn into_notes(self) -> VecDeque<String> {
        match self {
            CounterExample::Wildcard => VecDeque::new(),
            CounterExample::Literal(_) => VecDeque::new(),
            CounterExample::Note(s, next) => {
                let mut notes = next.into_notes();
                notes.push_front(s.clone());
                notes
            }
            CounterExample::Constructor(_, _, inner) => inner
                .into_iter()
                .flat_map(|ce| ce.into_notes())
                .collect::<VecDeque<_>>(),
        }
    }
}

impl Display for CounterExample {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            CounterExample::Wildcard => write!(f, "_"),
            CounterExample::Literal(s) => write!(f, "{}", s),
            CounterExample::Note(_, inner) => inner.fmt(f),
            CounterExample::Constructor(e, v, args) => {
                write!(f, "{}::{}", e, v)?;
                if !args.is_empty() {
                    write!(f, "(")?;
                    write!(
                        f,
                        "{}",
                        args.iter()
                            .map(|arg| format!("{}", arg))
                            .collect::<Vec<_>>()
                            .join(", ")
                    )?;
                    write!(f, ")")
                } else {
                    Ok(())
                }
            }
        }
    }
}

/// Returns true if it found a counter-example.
fn find_counterexample(
    context: &mut Context,
    loc: Loc,
    matrix: PatternMatrix,
    has_guards: bool,
) -> bool {
    fn make_wildcards(n: usize) -> Vec<CounterExample> {
        std::iter::repeat(CounterExample::Wildcard)
            .take(n)
            .collect()
    }

    // \mathcal{I} from Maranget. Warning for pattern matching. 1992.
    fn find_counterexample(
        context: &mut Context,
        matrix: PatternMatrix,
        arity: u32,
        ndx: &mut u32,
    ) -> Option<Vec<CounterExample>> {
        // println!("checking matrix");
        // matrix.print_verbose();
        let result = if matrix.patterns_empty() {
            None
        } else if matrix.is_empty() {
            Some(make_wildcards(arity as usize))
        } else if let Some(sp!(_, BuiltinTypeName_::Bool)) = matrix
            .tys
            .first()
            .unwrap()
            .value
            .unfold_to_builtin_type_name()
        {
            let literals = matrix.first_lits();
            assert!(literals.len() <= 2, "ICE match exhaustiveness failure");
            if literals.len() == 2 {
                // Saturated
                for lit in literals {
                    if let Some(counterexample) = find_counterexample(
                        context,
                        matrix.specialize_literal(&lit).1,
                        arity - 1,
                        ndx,
                    ) {
                        let lit_str = format!("{}", lit);
                        let result = [CounterExample::Literal(lit_str)]
                            .into_iter()
                            .chain(counterexample)
                            .collect();
                        return Some(result);
                    }
                }
                None
            } else {
                let (_, default) = matrix.default();
                if let Some(counterexample) = find_counterexample(context, default, arity - 1, ndx)
                {
                    if literals.is_empty() {
                        let result = [CounterExample::Wildcard]
                            .into_iter()
                            .chain(counterexample)
                            .collect();
                        Some(result)
                    } else {
                        let mut unused = BTreeSet::from([Value_::Bool(true), Value_::Bool(false)]);
                        for lit in literals {
                            unused.remove(&lit.value);
                        }
                        let result = [CounterExample::Literal(format!(
                            "{}",
                            unused.first().unwrap()
                        ))]
                        .into_iter()
                        .chain(counterexample)
                        .collect();
                        Some(result)
                    }
                } else {
                    None
                }
            }
        } else if let Some(sp!(_, _)) = matrix.tys[0].value.unfold_to_builtin_type_name() {
            // For all other non-literals, we don't consider a case where the constructors are
            // saturated.
            let literals = matrix.first_lits();
            let (_, default) = matrix.default();
            if let Some(counterexample) = find_counterexample(context, default, arity - 1, ndx) {
                if literals.is_empty() {
                    let result = [CounterExample::Wildcard]
                        .into_iter()
                        .chain(counterexample)
                        .collect();
                    Some(result)
                } else {
                    let n_id = format!("_{}", ndx);
                    *ndx += 1;
                    let lit_strs = literals
                        .into_iter()
                        .map(|lit| format!("{}", lit))
                        .collect::<Vec<_>>();
                    let lit_str = or_list_string(lit_strs);
                    let lit_msg = format!("When '{}' is not {}", n_id, lit_str);
                    let lit_ce =
                        CounterExample::Note(lit_msg, Box::new(CounterExample::Literal(n_id)));
                    let result = [lit_ce].into_iter().chain(counterexample).collect();
                    Some(result)
                }
            } else {
                None
            }
        } else {
            // println!("matrix types:");
            // for ty in &matrix.tys {
            //     ty.print_verbose();
            // }
            let (mident, datatype_name) = matrix.tys[0]
                .value
                .unfold_to_type_name()
                // .map(|name| {
                //     println!("name: {:#?}", name);
                //     name
                // })
                .and_then(|sp!(_, name)| name.datatype_name())
                .expect("ICE non-datatype type in head constructor fringe position");

            if context.is_struct(&mident, &datatype_name) {
                let (_, default) = matrix.default();
                if let Some(counterexample) = find_counterexample(context, default, arity - 1, ndx)
                {
                    let result = [CounterExample::Wildcard]
                        .into_iter()
                        .chain(counterexample)
                        .collect();
                    return Some(result);
                } else {
                    return None;
                }
            }

            let mut unmatched_variants = context
                .enum_variants(&mident, &datatype_name)
                .into_iter()
                .collect::<BTreeSet<_>>();

            let ctors = matrix.first_head_ctors();
            for ctor in ctors.keys() {
                unmatched_variants.remove(ctor);
            }
            if unmatched_variants.is_empty() {
                for (ctor, (ploc, arg_types)) in ctors {
                    let ctor_arity = arg_types.len() as u32;
                    let fringe_binders = context.make_imm_ref_match_binders(ploc, arg_types);
                    let bind_tys = fringe_binders
                        .iter()
                        .map(|(_, _, ty)| ty)
                        .collect::<Vec<_>>();
                    let (_, inner_matrix) = matrix.specialize(context, &ctor, bind_tys);
                    if let Some(mut counterexample) =
                        find_counterexample(context, inner_matrix, ctor_arity + arity - 1, ndx)
                    {
                        let ctor_args = counterexample
                            .drain(0..(ctor_arity as usize))
                            .collect::<Vec<_>>();
                        let output = [CounterExample::Constructor(datatype_name, ctor, ctor_args)]
                            .into_iter()
                            .chain(counterexample)
                            .collect();
                        return Some(output);
                    }
                }
                None
            } else {
                let (_, default) = matrix.default();
                if let Some(counterexample) = find_counterexample(context, default, arity - 1, ndx)
                {
                    if ctors.is_empty() {
                        // If we didn't match any head constructor, `_` is a reasonable
                        // counter-example entry.
                        let mut result = vec![CounterExample::Wildcard];
                        result.extend(&mut counterexample.into_iter());
                        Some(result)
                    } else {
                        let variant_name = unmatched_variants.first().unwrap();
                        let ctor_arity = context
                            .enum_variant_fields(&mident, &datatype_name, variant_name)
                            .unwrap()
                            .iter()
                            .count();
                        let args = make_wildcards(ctor_arity);
                        let result = [CounterExample::Constructor(
                            datatype_name,
                            *variant_name,
                            args,
                        )]
                        .into_iter()
                        .chain(counterexample)
                        .collect();
                        Some(result)
                    }
                } else {
                    // If we are missing a variant but everything else is fine, we're done.
                    None
                }
            }
        };
        // print!("result:");
        // match result {
        //     Some(ref n) => println!("{:#?}", n),
        //     None => println!("NONE"),
        // }
        // println!();
        result
    }

    // let result = fancy_i(context, matrix, 1);
    // match result {
    //     Some(ref n) => println!("{}", n[0]),
    //     None => println!("NON"),
    // }

    let mut ndx = 0;
    if let Some(mut counterexample) = find_counterexample(context, matrix, 1, &mut ndx) {
        // println!("counterexamples: {}", counterexample.len());
        // for ce in &counterexample {
        //     println!("{}", ce);
        // }
        assert!(counterexample.len() == 1);
        let counterexample = counterexample.remove(0);
        let msg = format!("Pattern '{}' not covered", counterexample);
        let mut diag = diag!(TypeSafety::IncompletePattern, (loc, msg));
        for note in counterexample.into_notes() {
            diag.add_note(note);
        }
        if has_guards {
            diag.add_note("Match arms with guards are not considered for coverage.");
        }
        context.env.add_diag(diag);
        true
    } else {
        false
    }
}

//**************************************************************************************************
// Debug Print
//**************************************************************************************************

impl AstDebug for PatternMatrix {
    fn ast_debug(&self, w: &mut AstWriter) {
        for arm in &self.patterns {
            let PatternArm { pat, guard, arm } = arm;
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
