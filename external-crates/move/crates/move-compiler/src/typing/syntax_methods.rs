// Copyright (c) The Move Contributors
// SPDX-License-Identifier: Apache-2.0

//! This module verifies the usage of the "syntax method" functions. These functions are declared
//! as 'syntax' but have not been ensured to be type-compatible or otherwise adhere to our
//! trait-like constraints around their definitions. We process them here, using typing machinery
//! to ensure the are.

use crate::{
    diag,
    expansion::ast::AbilitySet,
    expansion::ast::ModuleIdent,
    ice,
    naming::ast::{self as N, IndexSyntaxMethods, SyntaxMethod},
    typing::core::{self, Context},
};
use move_ir_types::location::*;

//-------------------------------------------------------------------------------------------------
// Validation
//-------------------------------------------------------------------------------------------------

pub fn validate_syntax_methods(
    context: &mut Context,
    _mident: &ModuleIdent,
    module: &mut N::ModuleDefinition,
) {
    let methods = &mut module.syntax_methods;
    for (_, entry) in methods.iter_mut() {
        if let Some(index) = &mut entry.index {
            let IndexSyntaxMethods { index, index_mut } = &mut **index;
            if let (Some(index_defn), Some(index_mut_defn)) = (index.as_ref(), index_mut.as_ref()) {
                if !validate_index_syntax_methods(context, index_defn, index_mut_defn) {
                    // If we didn't validate they wre comptaible, we remove the mut one to avoid more
                    // typing issues later.
                    assert!(context.env.has_errors());
                    *index_mut = None;
                }
            }
        }
    }
}

fn validate_index_syntax_methods(
    context: &mut Context,
    index: &SyntaxMethod,
    index_mut: &SyntaxMethod,
) -> bool {
    let index_ann_loc = index.kind.loc;
    let (index_module, index_fn) = &index.target_function;
    let (index_mut_module, index_mut_fn) = &index_mut.target_function;

    let index_finfo = context.function_info(index_module, index_fn).clone();
    let mut_finfo = context
        .function_info(index_mut_module, index_mut_fn)
        .clone();

    if index_finfo.signature.type_parameters.len() != mut_finfo.signature.type_parameters.len() {
        let index_msg = format!(
            "This index function expects {} type arguments",
            index_finfo.signature.type_parameters.len()
        );
        let index_mut_msg = format!(
            "This mutable index function expects {} type arguments",
            mut_finfo.signature.type_parameters.len()
        );
        let mut diag = diag!(
            TypeSafety::IncompatibleSyntaxMethods,
            (index.loc, index_msg),
            (index_mut.loc, index_mut_msg),
        );
        diag.add_note(
            "Index operations on the same type must take the name number of type arguments",
        );
        context.env.add_diag(diag);
        return false;
    }

    if index_finfo.signature.parameters.len() != mut_finfo.signature.parameters.len() {
        let index_msg = format!(
            "This index function expects {} parameters",
            index_finfo.signature.parameters.len()
        );
        let index_mut_msg = format!(
            "This mutable index function expects {} parameters ",
            mut_finfo.signature.parameters.len()
        );
        let mut diag = diag!(
            TypeSafety::IncompatibleSyntaxMethods,
            (index.loc, index_msg),
            (index_mut.loc, index_mut_msg),
        );
        diag.add_note("Index operations on the same type must take the name number of parameters");
        context.env.add_diag(diag);
        return false;
    }

    // Now we simply want to skolomize the types and make sure they are the same. To do this, we
    // instantiate the type variables and ground them both to new type parameters. Finally, we walk
    // both types and make sure they agree. Note that  we discard any constraints introduced here
    // because those will be checked later when the index functions are actually used.

    let prev_constraints = std::mem::take(&mut context.constraints);

    let mut valid = true;

    let mut subst = core::Subst::empty();

    let index_ty = core::make_function_type(context, index_ann_loc, index_module, index_fn, None);
    let index_mut_ty =
        core::make_function_type(context, index_ann_loc, index_mut_module, index_mut_fn, None);

    for (ndx, (index_tyarg, index_mut_tyarg)) in index_ty
        .ty_args
        .iter()
        .zip(index_mut_ty.ty_args.iter())
        .enumerate()
    {
        if let Ok((subst_, _)) = core::join(subst.clone(), index_tyarg, index_mut_tyarg) {
            subst = subst_;
        } else {
            context
                .env
                .add_diag(ice!((index.loc, "Failed in validating syntax methods")));
            valid = false;
        }
        // This essentially fakes skolomeziation so that we can fail when the types aren't the same.
        let id = N::TParamID::next();
        let user_specified_name = sp(index_tyarg.loc, format!("_{}", ndx).into());
        let tparam = N::TParam {
            id,
            user_specified_name,
            abilities: AbilitySet::all(index_ann_loc),
        };
        if let Ok((subst_, _)) = core::join(
            subst.clone(),
            index_tyarg,
            &sp(index_tyarg.loc, N::Type_::Param(tparam)),
        ) {
            subst = subst_;
        } else {
            context
                .env
                .add_diag(ice!((index.loc, "Failed in validating syntax methods")));
            valid = false;
        }
    }

    let mut param_tys = index_ty
        .params
        .iter()
        .zip(index_mut_ty.params.iter())
        .map(|((_, t1), (_, t2))| (t1, t2))
        .enumerate();

    // The first one is a subtype because we want to ensure the `&mut` param is a subtype of the
    // `&` param. We already ensured they were both references of the appropriate shape in naming,
    // so this is a bit redundant.
    if let Some((ndx, (subject_ref_type, subject_mut_ref_type))) = param_tys.next() {
        if let Ok((subst_, _)) =
            core::subtype(subst.clone(), subject_mut_ref_type, subject_ref_type)
        {
            subst = subst_;
        } else {
            let (_, _, index_type) = &index_finfo.signature.parameters[ndx];
            let (_, _, mut_type) = &mut_finfo.signature.parameters[ndx];
            // This case shouldn't really be reachable, but we might as well provide an error.
            let index_msg = format!(
                "This index function subject has type {}",
                ty_str(index_type)
            );
            let mut_msg = format!(
                "This mutable index function subject has type {}",
                ty_str(mut_type)
            );
            let mut diag = diag!(
                TypeSafety::IncompatibleSyntaxMethods,
                (index_type.loc, index_msg),
                (mut_type.loc, mut_msg)
            );
            diag.add_note(
                "These functions must take the same subject type, differing only by mutability.",
            );
            context.env.add_diag(diag);
            valid = false;
        }
    } else {
        valid = false;
    }

    // We ensure the rest of the parameters match exactly.
    for (ndx, (ptype, mut_ptype)) in param_tys {
        if let Ok((subst_, _)) = core::invariant(subst.clone(), ptype, mut_ptype) {
            subst = subst_;
        } else {
            let (_, _, index_type) = &index_finfo.signature.parameters[ndx];
            let (_, _, mut_type) = &mut_finfo.signature.parameters[ndx];
            let index_msg = format!("This index function expects type {}", ty_str(index_type));
            let mut_msg = format!(
                "This mutable index function expects type {}",
                ty_str(mut_type)
            );
            let mut diag = diag!(
                TypeSafety::IncompatibleSyntaxMethods,
                (index_type.loc, index_msg),
                (mut_type.loc, mut_msg)
            );
            diag.add_note("Index operation non-subject parameter types must match exactly");
            context.env.add_diag(diag);
            valid = false;
        }
    }

    // Similar to the subject type, we ensure the return types are the same. We already checked
    // that they are appropriately-shaped references, and now we ensure they refer to the same type
    // under the reference.
    if core::subtype(subst, &index_mut_ty.return_, &index_ty.return_).is_err() {
        let sp!(index_loc, index_type) = &index_finfo.signature.return_type;
        let sp!(mut_loc, mut_type) = &mut_finfo.signature.return_type;
        let index_msg = format!("This index function returns type {}", ty_str_(index_type));
        let mut_msg = format!(
            "This mutable index function returns type {}",
            ty_str_(mut_type)
        );
        let mut diag = diag!(
            TypeSafety::IncompatibleSyntaxMethods,
            (*index_loc, index_msg),
            (*mut_loc, mut_msg)
        );
        diag.add_note("These functions must return the same type, differing only by mutability.");
        context.env.add_diag(diag);
        valid = false;
    }

    let _ = std::mem::replace(&mut context.constraints, prev_constraints);

    valid
}

// Error printing helpers

fn ty_str(ty: &N::Type) -> String {
    core::error_format(ty, &core::Subst::empty())
}

fn ty_str_(ty: &N::Type_) -> String {
    core::error_format_(ty, &core::Subst::empty())
}
