// Copyright (c) The Diem Core Contributors
// Copyright (c) The Move Contributors
// SPDX-License-Identifier: Apache-2.0

use crate::{
    debug_display, diag,
    editions::FeatureGate,
    expansion::ast::Value_,
    ice,
    naming::ast::{
        ANYTHING_TYPE, BuiltinTypeName_, FunctionSignature, Type, TypeInner, TypeName_,
        UNRESOLVED_ERROR_TYPE,
    },
    parser::ast::Ability_,
    shared::{ide::IDEAnnotation, string_utils::debug_print},
    typing::{
        ast::{self as T},
        core::{self, Context},
    },
};
use move_core_types::u256::U256;
use move_ir_types::location::*;
use move_proc_macros::growing_stack;

//**************************************************************************************************
// Functions
//**************************************************************************************************

pub fn function_body_(context: &mut Context, b_: &mut T::FunctionBody_) {
    match b_ {
        T::FunctionBody_::Native | T::FunctionBody_::Macro => (),
        T::FunctionBody_::Defined(es) => sequence(context, es),
    }
}

pub fn function_signature(context: &mut Context, sig: &mut FunctionSignature) {
    for (_, _, st) in &mut sig.parameters {
        type_(context, st);
    }
    type_(context, &mut sig.return_type);
}

//**************************************************************************************************
// Types
//**************************************************************************************************

fn expected_types(context: &mut Context, ss: &mut [Option<Type>]) {
    for st_opt in ss.iter_mut().flatten() {
        type_(context, st_opt);
    }
}

fn types(context: &mut Context, ss: &mut Vec<Type>) {
    for st in ss {
        type_(context, st);
    }
}

pub fn type_(context: &mut Context, ty: &mut Type) {
    fn types_recur(context: &mut Context, tys: &[Type]) -> (Vec<Type>, bool) {
        let mut changed = false;
        let mut new_tys = vec![];
        for t in tys.iter() {
            let (new_t, t_changed) = type_recur(context, t);
            changed |= t_changed;
            new_tys.push(new_t);
        }
        (new_tys, changed)
    }

    #[growing_stack]
    fn type_recur(context: &mut Context, ty: &Type) -> (Type, bool) {
        use TypeInner as TI;
        match ty.value.inner() {
            TI::Anything | TI::UnresolvedError | TI::Param(_) | TI::Unit | TI::Void => {
                (ty.clone(), false)
            }
            TI::Ref(mut_, b) => {
                let (b, changed) = type_recur(context, b);
                if changed {
                    (sp(ty.loc, TI::Ref(*mut_, b).into()), true)
                } else {
                    (ty.clone(), false)
                }
            }
            tv @ TI::Var(_) => {
                debug_print!(context.debug().type_elaboration, ("before" => tv));
                let replacement = core::unfold_type(&context.subst, ty);
                debug_print!(context.debug().type_elaboration, ("resolved" => replacement));
                let loc = replacement.loc;
                let replacement = match replacement.value.inner() {
                    TI::Var(_) => {
                        let diag = ice!((ty.loc, "ICE unfold_type failed to expand type inf. var"));
                        context.add_diag(diag);
                        sp(loc, UNRESOLVED_ERROR_TYPE.clone())
                    }
                    TI::Anything => {
                        let msg = "Could not infer this type. Try adding an annotation";
                        context.add_diag(diag!(TypeSafety::UninferredType, (ty.loc, msg)));
                        sp(loc, UNRESOLVED_ERROR_TYPE.clone())
                    }
                    TI::Fun(_, _) if !context.in_macro_function => {
                        // catch this here for better location infomration (the tvar instead of the fun)
                        unexpected_lambda_type(context, ty.loc);
                        sp(loc, UNRESOLVED_ERROR_TYPE.clone())
                    }
                    _ => replacement,
                };
                let (replacement, _changed) = type_recur(context, &replacement);
                debug_print!(context.debug().type_elaboration, ("after" => replacement));
                (replacement, true)
            }
            TI::Apply(Some(abilities), name @ sp!(_, TypeName_::Builtin(_)), tys) => {
                let (new_tys, tys_changed) = types_recur(context, tys);
                if tys_changed {
                    let new_ty = TI::Apply(Some(abilities.clone()), *name, new_tys);
                    (sp(ty.loc, new_ty.into()), true)
                } else {
                    (ty.clone(), false)
                }
            }
            TI::Apply(Some(_), _, _) => {
                let diag = ice!((
                    ty.loc,
                    format!("ICE expanding pre-expanded type {}", debug_display!(ty))
                ));
                context.add_diag(diag);
                (sp(ty.loc, UNRESOLVED_ERROR_TYPE.clone()), true)
            }
            TI::Apply(None, name, tys) => {
                let abilities = core::infer_abilities(context.info(), &context.subst, ty);
                let (new_tys, _) = types_recur(context, tys);
                let ty = sp(
                    ty.loc,
                    TI::Apply(Some(abilities.clone()), *name, new_tys).into(),
                );
                (ty, true)
            }
            TI::Fun(args, result) => {
                if context.in_macro_function {
                    let mut changed = false;

                    let (new_args, args_changed) = types_recur(context, args);
                    changed |= args_changed;
                    let (new_result, result_changed) = type_recur(context, result);
                    changed |= result_changed;

                    if changed {
                        (sp(ty.loc, TI::Fun(new_args, new_result).into()), true)
                    } else {
                        (ty.clone(), false)
                    }
                } else {
                    unexpected_lambda_type(context, ty.loc);
                    (sp(ty.loc, UNRESOLVED_ERROR_TYPE.clone()), true)
                }
            }
        }
    }

    let (new_ty, changed) = type_recur(context, ty);
    if changed {
        *ty = new_ty;
    }
}

fn unexpected_lambda_type(context: &mut Context, loc: Loc) {
    if context.check_feature(context.current_package(), FeatureGate::MacroFuns, loc) {
        let msg = "Unexpected lambda type. \
            Lambdas can only be used with 'macro' functions, as parameters or direct arguments";
        context.add_diag(diag!(TypeSafety::UnexpectedFunctionType, (loc, msg)));
    }
}

//**************************************************************************************************
// Expressions
//**************************************************************************************************

#[growing_stack]
fn sequence(context: &mut Context, (_, seq): &mut T::Sequence) {
    for item in seq {
        sequence_item(context, item)
    }
}

#[growing_stack]
fn sequence_item(context: &mut Context, item: &mut T::SequenceItem) {
    use T::SequenceItem_ as S;
    match &mut item.value {
        S::Seq(te) => exp(context, te),

        S::Declare(tbind) => lvalues(context, tbind),
        S::Bind(tbind, tys, te) => {
            lvalues(context, tbind);
            expected_types(context, tys);
            exp(context, te)
        }
    }
}

#[growing_stack]
pub fn exp(context: &mut Context, e: &mut T::Exp) {
    use T::UnannotatedExp_ as E;
    use TypeInner as TI;

    fn expand_type_for_errors(context: &mut Context, ty: &Type) {
        let mut unfolded_type = core::unfold_type(&context.subst, ty);
        match unfolded_type.value.inner() {
            TI::Anything => (),
            _ => {
                // report errors if there is an uninferred type argument somewhere
                type_(context, &mut unfolded_type);
            }
        }
    }

    match &e.exp.value {
        // dont expand the type for return, abort, break, or continue
        E::Give(_, _) | E::Continue(_) | E::Return(_) | E::Abort(_) => {
            expand_type_for_errors(context, &e.ty);
            e.ty = sp(e.ty.loc, ANYTHING_TYPE.clone())
        }
        E::Loop {
            has_break: false, ..
        } => {
            expand_type_for_errors(context, &e.ty);
            e.ty = sp(e.ty.loc, ANYTHING_TYPE.clone())
        }
        _ => type_(context, &mut e.ty),
    }
    match &mut e.exp.value {
        E::Use(v) => {
            let from_user = false;
            let var = *v;
            let abs = core::infer_abilities(context.info(), &context.subst, &e.ty);
            e.exp.value = if abs.has_ability_(Ability_::Copy) {
                E::Copy { from_user, var }
            } else {
                E::Move { from_user, var }
            }
        }
        E::Value(sp!(vloc, Value_::InferredNum(v))) => {
            if let Some(value) = inferred_numerical_value(context, e.exp.loc, *v, &e.ty) {
                e.exp.value = E::Value(sp(*vloc, value));
            } else {
                e.exp.value = E::UnresolvedError
            }
        }
        E::Value(sp!(vloc, Value_::InferredString(v))) => {
            if let Some(exp) = inferred_string_value(context, e.exp.loc, *vloc, v.clone(), &e.ty) {
                *e = exp
            } else {
                e.exp.value = E::UnresolvedError
            }
        }

        E::Unit { .. }
        | E::Value(_)
        | E::Constant(_, _)
        | E::Move { .. }
        | E::Copy { .. }
        | E::BorrowLocal(_, _)
        | E::Continue(_)
        | E::ErrorConstant { .. }
        | E::UnresolvedError => (),

        E::ModuleCall(call) => module_call(context, call),
        E::Builtin(b, args) => {
            builtin_function(context, b);
            exp(context, args);
        }
        E::Vector(_vec_loc, _n, ty_arg, args) => {
            type_(context, ty_arg);
            exp(context, args);
        }

        E::IfElse(eb, et, ef_opt) => {
            exp(context, eb);
            exp(context, et);
            if let Some(ef) = ef_opt {
                exp(context, ef)
            }
        }
        E::Match(esubject, arms) => {
            exp(context, esubject);
            for arm in arms.value.iter_mut() {
                match_arm(context, arm);
            }
        }
        E::VariantMatch(subject, _, arms) => {
            context.add_diag(ice!((
                e.exp.loc,
                "shouldn't find variant match before match compilation"
            )));
            exp(context, subject);
            for (_, rhs) in arms {
                exp(context, rhs);
            }
        }
        E::While(_, eb, eloop) => {
            exp(context, eb);
            exp(context, eloop);
        }
        E::Loop { body: eloop, .. } => exp(context, eloop),
        E::NamedBlock(_, seq) => sequence(context, seq),
        E::Block(seq) => sequence(context, seq),
        E::Assign(assigns, tys, er) => {
            lvalues(context, assigns);
            expected_types(context, tys);
            exp(context, er);
        }

        E::Return(base_exp)
        | E::Abort(base_exp)
        | E::Give(_, base_exp)
        | E::Dereference(base_exp)
        | E::UnaryExp(_, base_exp)
        | E::Borrow(_, base_exp, _)
        | E::TempBorrow(_, base_exp) => exp(context, base_exp),
        E::Mutate(el, er) => {
            exp(context, el);
            exp(context, er)
        }
        E::BinopExp(el, _, operand_ty, er) => {
            exp(context, el);
            exp(context, er);
            type_(context, operand_ty);
        }

        E::Pack(_, _, bs, fields) => {
            types(context, bs);
            for (_, _, (_, (bt, fe))) in fields.iter_mut() {
                type_(context, bt);
                exp(context, fe)
            }
        }
        E::PackVariant(_, _, _, bs, fields) => {
            types(context, bs);
            for (_, _, (_, (bt, fe))) in fields.iter_mut() {
                type_(context, bt);
                exp(context, fe)
            }
        }
        E::ExpList(el) => exp_list(context, el),
        E::Cast(el, rhs_ty) | E::Annotate(el, rhs_ty) => {
            exp(context, el);
            type_(context, rhs_ty);
        }
    }
}

fn inferred_numerical_value(
    context: &mut Context,
    eloc: Loc,
    value: U256,
    ty: &Type,
) -> Option<Value_> {
    use BuiltinTypeName_ as BT;
    let bt = match ty.value.builtin_name() {
        Some(sp!(_, bt)) if bt.is_numeric() => bt,
        _ => panic!("ICE inferred num failed {:?}", &ty.value),
    };
    let u8_max = U256::from(u8::MAX);
    let u16_max = U256::from(u16::MAX);
    let u32_max = U256::from(u32::MAX);
    let u64_max = U256::from(u64::MAX);
    let u128_max = U256::from(u128::MAX);
    let u256_max = U256::max_value();
    let max = match bt {
        BT::U8 => u8_max,
        BT::U16 => u16_max,
        BT::U32 => u32_max,
        BT::U64 => u64_max,
        BT::U128 => u128_max,
        BT::U256 => u256_max,
        BT::Address | BT::Signer | BT::Vector | BT::Bool => unreachable!(),
    };
    if value > max {
        let msg = format!(
            "Expected a literal of type '{}', but the value is too large.",
            bt
        );
        let fix_bt = if value > u128_max {
            BT::U256
        } else if value > u64_max {
            BT::U128
        } else if value > u32_max {
            BT::U64
        } else if value > u16_max {
            BT::U32
        } else {
            assert!(value > u8_max);
            BT::U16
        };

        let fix = format!(
            "Annotating the literal might help inference: '{value}{type}'",
            type=fix_bt,
        );
        context.add_diag(diag!(
            TypeSafety::InvalidNum,
            (eloc, "Invalid numerical literal"),
            (ty.loc, msg),
            (eloc, fix),
        ));
        None
    } else {
        let value_ = match bt {
            BT::U8 => Value_::U8(value.down_cast_lossy()),
            BT::U16 => Value_::U16(value.down_cast_lossy()),
            BT::U32 => Value_::U32(value.down_cast_lossy()),
            BT::U64 => Value_::U64(value.down_cast_lossy()),
            BT::U128 => Value_::U128(value.down_cast_lossy()),
            BT::U256 => Value_::U256(value),
            BT::Address | BT::Signer | BT::Vector | BT::Bool => unreachable!(),
        };
        Some(value_)
    }
}

fn inferred_string_value(
    context: &mut Context,
    _eloc: Loc,
    value_loc: Loc,
    value: Vec<u8>,
    ty: &Type,
) -> Option<T::Exp> {
    use BuiltinTypeName_ as BT;
    use T::UnannotatedExp_ as E;
    use TypeInner as TI;
    use TypeName_ as TN;
    let diag_note =
        || "String literals must be linked against the standard library to be constructed";

    match &ty.value.inner() {
        TI::Apply(_, sp!(_, TN::Builtin(sp!(_, BT::Vector))), args)
            if args.len() == 1
                && matches!(
                    args[0].value.inner(),
                    TI::Apply(_, sp!(_, TN::Builtin(sp!(_, BT::U8))), _)
                ) =>
        {
            Some(T::exp(
                ty.clone(),
                sp(value_loc, E::Value(sp(value_loc, Value_::Bytearray(value)))),
            ))
        }
        TI::Apply(_, sp!(_, name), args) if args.is_empty() => {
            let possibles = context.outer.get_stdlib_string_info();

            let possible = possibles
                .iter()
                .map(|(str_ty, ctor_opt, validator)| {
                    let mut str_ty = str_ty.clone();
                    type_(context, &mut str_ty);
                    (str_ty, ctor_opt, validator)
                })
                .find(|(str_ty, _, _)| core::subtype_check(str_ty, ty));

            let Some((_, str_ctor_opt, validator)) = possible else {
                let msg = format!("Could not find library definition for type '{}'", name);
                let mut diag = diag!(TypeSafety::InvalidString, (value_loc, msg));
                diag.add_note(diag_note());
                context.add_diag(diag);
                return None;
            };

            let Some((module, ctor)) = *str_ctor_opt else {
                let msg = format!("Could not find constructor for type '{}'", name);
                let mut diag = diag!(TypeSafety::InvalidString, (value_loc, msg));
                diag.add_note(diag_note());
                context.add_diag(diag);
                return None;
            };

            // Check the value format
            if let Err(err_msg) = validator(&Value_::InferredString(value.clone())) {
                let mut diag = diag!(TypeSafety::InvalidString, (value_loc, err_msg));
                diag.add_note(diag_note());
                context.add_diag(diag);
                return None;
            }

            // Build up the typed input value
            let value_ = Value_::Bytearray(value);
            let Some(mut bytearray_ty) = value_.type_(value_loc) else {
                context.add_diag(ice!((value_loc, "Could not get bytearray type")));
                return None;
            };
            type_(context, &mut bytearray_ty); // Expand the vector type
            let value = sp(value_loc, E::Value(sp(value_loc, value_)));
            let value_exp = T::Exp {
                ty: bytearray_ty.clone(),
                exp: value,
            };

            // Create the call itself
            let module_call = T::ModuleCall {
                module,
                name: ctor,
                type_arguments: vec![],
                arguments: Box::new(value_exp),
                parameter_types: vec![bytearray_ty],
                method_name: None,
            };
            let call = T::UnannotatedExp_::ModuleCall(Box::new(module_call));
            let exp = T::exp(ty.clone(), sp(value_loc, call));
            Some(exp)
        }
        _ => panic!("ICE inferred string failed {:?}", &ty.value),
    }
}

fn match_arm(context: &mut Context, sp!(_, arm_): &mut T::MatchArm) {
    pat(context, &mut arm_.pattern);
    for (_, ty) in arm_.binders.iter_mut() {
        type_(context, ty);
    }
    if let Some(guard) = arm_.guard.as_mut() {
        exp(context, guard)
    }
    exp(context, &mut arm_.rhs);
}

fn pat(context: &mut Context, p: &mut T::MatchPattern) {
    use T::UnannotatedPat_ as P;
    use TypeInner as TI;

    type_(context, &mut p.ty);
    match &mut p.pat.value {
        P::Variant(_, _, _, bts, fields) | P::BorrowVariant(_, _, _, _, bts, fields) => {
            types(context, bts);
            for (_, _, (_, (bt, innerb))) in fields.iter_mut() {
                type_(context, bt);
                pat(context, innerb)
            }
        }
        P::Struct(_, _, bts, fields) | P::BorrowStruct(_, _, _, bts, fields) => {
            types(context, bts);
            for (_, _, (_, (bt, innerb))) in fields.iter_mut() {
                type_(context, bt);
                pat(context, innerb)
            }
        }
        P::Literal(sp!(vloc, Value_::InferredNum(v))) => {
            let num_ty: &Type = match &p.ty.value.inner() {
                TI::Ref(_, inner) => inner,
                TI::Unit
                | TI::Param(_)
                | TI::Apply(_, _, _)
                | TI::Fun(_, _)
                | TI::Var(_)
                | TI::Anything
                | TI::UnresolvedError
                | TI::Void => &p.ty,
            };
            if let Some(value) = inferred_numerical_value(context, p.pat.loc, *v, num_ty) {
                p.pat.value = P::Literal(sp(*vloc, value));
            } else {
                p.pat.value = P::ErrorPat;
            }
        }
        P::Or(lhs, rhs) => {
            pat(context, lhs);
            pat(context, rhs);
        }
        P::At(_var, inner) => pat(context, inner),
        P::Constant(_, _) | P::ErrorPat | P::Literal(_) | P::Binder(_, _) | P::Wildcard => (),
    }
}

fn lvalues(context: &mut Context, binds: &mut T::LValueList) {
    for b in &mut binds.value {
        lvalue(context, b)
    }
}

fn lvalue(context: &mut Context, b: &mut T::LValue) {
    use T::LValue_ as L;
    use TypeInner as TI;

    match &mut b.value {
        L::Ignore => (),
        L::Var {
            ty,
            unused_binding: true,
            ..
        } => {
            // silence type inference error for unused bindings
            if let TI::Var(_) = &ty.value.inner() {
                let replacement = core::unfold_type(&context.subst, ty);
                if let TI::Anything = replacement.value.inner() {
                    b.value = L::Ignore;
                    return;
                }
            }
            type_(context, ty);
        }
        L::Var { ty, .. } => {
            type_(context, ty);
        }
        L::BorrowUnpack(_, _, _, bts, fields) | L::Unpack(_, _, bts, fields) => {
            types(context, bts);
            for (_, _, (_, (bt, innerb))) in fields.iter_mut() {
                type_(context, bt);
                lvalue(context, innerb)
            }
        }
        L::BorrowUnpackVariant(..) | L::UnpackVariant(..) => {
            panic!("ICE shouldn't occur before match expansions")
        }
    }
}

fn module_call(context: &mut Context, call: &mut T::ModuleCall) {
    types(context, &mut call.type_arguments);
    exp(context, &mut call.arguments);
    types(context, &mut call.parameter_types)
}

fn builtin_function(context: &mut Context, b: &mut T::BuiltinFunction) {
    use T::BuiltinFunction_ as B;
    match &mut b.value {
        B::Freeze(bt) => {
            type_(context, bt);
        }
        B::Assert(_) => (),
    }
}

fn exp_list(context: &mut Context, items: &mut Vec<T::ExpListItem>) {
    for item in items {
        exp_list_item(context, item)
    }
}

fn exp_list_item(context: &mut Context, item: &mut T::ExpListItem) {
    use T::ExpListItem as I;
    match item {
        I::Single(e, st) => {
            exp(context, e);
            type_(context, st);
        }
        I::Splat(_, e, ss) => {
            exp(context, e);
            types(context, ss);
        }
    }
}

//**************************************************************************************************
// IDE Information
//**************************************************************************************************

pub fn ide_annotation(context: &mut Context, annotation: &mut IDEAnnotation) {
    match annotation {
        IDEAnnotation::MacroCallInfo(info) => {
            for t in info.type_arguments.iter_mut() {
                type_(context, t);
            }
            for t in info.by_value_args.iter_mut() {
                sequence_item(context, t);
            }
        }
        IDEAnnotation::ExpandedLambda => (),
        IDEAnnotation::DotAutocompleteInfo(info) => {
            for (_, t) in info.fields.iter_mut() {
                type_(context, t);
            }
        }
        IDEAnnotation::MissingMatchArms(_) => (),
        IDEAnnotation::EllipsisMatchEntries(_) => (),
        IDEAnnotation::PathAutocompleteInfo(_) => (),
    }
}
