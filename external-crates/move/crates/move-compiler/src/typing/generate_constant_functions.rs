// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

//! Synthesizes `public(package)` getter functions for constants that are used in the function
//! bodies of other modules. Constants live in the constant pool of their defining module, so a
//! cross-module use cannot be compiled as a constant load; instead it is lowered (during HLIR
//! translation) to a call of a getter function, synthesized here in the defining module. The
//! getters are created on demand: a constant only referenced within its own module (or only in
//! other modules' constant definitions, which are resolved by constant folding) gets none.
//!
//! This runs at the end of typing, after macro expansion, so that a constant reference in a macro
//! body that expands into another module is correctly seen as a cross-module use.

use std::collections::{BTreeMap, BTreeSet, VecDeque};

use move_ir_types::location::*;
use move_proc_macros::growing_stack;
use move_symbol_pool::Symbol;

use crate::{
    diagnostics::filter,
    editions::FeatureGate,
    expansion::ast::{Friend, ModuleIdent, Visibility},
    naming::ast::{self as N, UseFuns},
    parser::ast::{ConstantName, DocComment, FunctionName},
    shared::{CompilationEnv, unique_map::UniqueMap},
    typing::ast as T,
};

//**************************************************************************************************
// Entry
//**************************************************************************************************

pub fn program(env: &CompilationEnv, modules: &mut UniqueMap<ModuleIdent, T::ModuleDefinition>) {
    let needed = needed_getters(env, modules);
    for (mident, uses) in needed {
        let mdef = modules.get_mut(&mident).unwrap();
        synthesize_getters(mident, mdef, uses);
    }
}

//**************************************************************************************************
// Collection
//**************************************************************************************************

/// The cross-module uses of a module's constants: the constants needing getters, and the using
/// modules, which must become friends of the defining module (`public(package)` visibility is
/// friend visibility in the compiled module)
#[derive(Default)]
struct CrossModuleUses {
    constants: BTreeSet<ConstantName>,
    users: BTreeMap<ModuleIdent, Loc>,
}

struct Context<'a> {
    env: &'a CompilationEnv,
    modules: &'a UniqueMap<ModuleIdent, T::ModuleDefinition>,
    current_module: ModuleIdent,
    /// Constants used from a module other than their defining one, keyed by defining module
    needed: BTreeMap<ModuleIdent, CrossModuleUses>,
}

impl Context<'_> {
    fn add_constant_use(&mut self, m: ModuleIdent, c: ConstantName, loc: Loc) {
        if m == self.current_module {
            return;
        }
        // Uses of non-'public(package)' constants, cross-package uses, and uses without the
        // feature enabled were rejected during typing, so no getter is synthesized for them.
        let Some(defining_mdef) = self.modules.get(&m) else {
            return;
        };
        let Some(cdef) = defining_mdef.constants.get(&c) else {
            return;
        };
        if !matches!(cdef.visibility, Visibility::Package(_)) {
            return;
        }
        let use_mdef = self.modules.get(&self.current_module).unwrap();
        if defining_mdef.package_name != use_mdef.package_name {
            return;
        }
        if !self.env.supports_feature(
            defining_mdef.package_name,
            FeatureGate::CrossModuleConstants,
        ) {
            return;
        }
        let uses = self.needed.entry(m).or_default();
        uses.constants.insert(c);
        uses.users.entry(self.current_module).or_insert(loc);
    }
}

fn needed_getters(
    env: &CompilationEnv,
    modules: &UniqueMap<ModuleIdent, T::ModuleDefinition>,
) -> BTreeMap<ModuleIdent, CrossModuleUses> {
    let mut needed = BTreeMap::new();
    for (mident, mdef) in modules.key_cloned_iter() {
        let mut context = Context {
            env,
            modules,
            current_module: mident,
            needed,
        };
        for (_, _, fdef) in &mdef.functions {
            // Macro bodies are not stored in the typed AST -- their expansions appear inline in
            // their callers, which are covered here.
            if let T::FunctionBody_::Defined(seq) = &fdef.body.value {
                sequence(&mut context, seq);
            }
        }
        needed = context.needed;
    }
    needed
}

fn sequence(context: &mut Context, (_, seq): &T::Sequence) {
    use T::SequenceItem_ as SI;
    for sp!(_, item_) in seq {
        match item_ {
            SI::Seq(e) => exp(context, e),
            SI::Declare(_) => (),
            SI::Bind(_, _, e) => exp(context, e),
        }
    }
}

#[growing_stack]
fn exp(context: &mut Context, e: &T::Exp) {
    use T::UnannotatedExp_ as E;
    match &e.exp.value {
        E::Constant(m, c) => context.add_constant_use(*m, *c, e.exp.loc),

        E::ModuleCall(c) => exp(context, &c.arguments),
        E::Builtin(_, e)
        | E::Vector(_, _, _, e)
        | E::Return(e)
        | E::Abort(e)
        | E::Give(_, e)
        | E::Dereference(e)
        | E::UnaryExp(_, e)
        | E::Borrow(_, e, _)
        | E::TempBorrow(_, e)
        | E::Cast(e, _)
        | E::Annotate(e, _) => exp(context, e),
        E::IfElse(e1, e2, e3_opt) => {
            exp(context, e1);
            exp(context, e2);
            if let Some(e3) = e3_opt {
                exp(context, e3);
            }
        }
        E::Match(esubject, arms) => {
            exp(context, esubject);
            for sp!(_, arm) in &arms.value {
                pat(context, &arm.pattern);
                if let Some(guard) = arm.guard.as_ref() {
                    exp(context, guard)
                }
                exp(context, &arm.rhs);
            }
        }
        E::VariantMatch(esubject, _, arms) => {
            exp(context, esubject);
            for (_, e) in arms {
                exp(context, e);
            }
        }
        E::While(_, e1, e2) | E::Mutate(e1, e2) | E::BinopExp(e1, _, _, e2) => {
            exp(context, e1);
            exp(context, e2);
        }
        E::Loop { body, .. } => exp(context, body),
        E::NamedBlock(_, seq) | E::Block(seq) => sequence(context, seq),
        E::Assign(_, _, e) => exp(context, e),
        E::Pack(_, _, _, fields) | E::PackVariant(_, _, _, _, fields) => {
            for (_, _, (_, (_, e))) in fields {
                exp(context, e)
            }
        }
        E::ExpList(list) => {
            for l in list {
                match l {
                    T::ExpListItem::Single(e, _) => exp(context, e),
                    T::ExpListItem::Splat(_, e, _) => exp(context, e),
                }
            }
        }

        E::Unit { .. }
        | E::Value(_)
        | E::Move { .. }
        | E::Copy { .. }
        | E::Use(_)
        | E::Continue(_)
        | E::BorrowLocal(..)
        | E::ErrorConstant { .. }
        | E::UnresolvedError => (),
    }
}

#[growing_stack]
fn pat(context: &mut Context, p: &T::MatchPattern) {
    use T::UnannotatedPat_ as P;
    match &p.pat.value {
        P::Constant(m, c) => context.add_constant_use(*m, *c, p.pat.loc),
        P::Variant(_, _, _, _, fields)
        | P::BorrowVariant(_, _, _, _, _, fields)
        | P::Struct(_, _, _, fields)
        | P::BorrowStruct(_, _, _, _, fields) => {
            for (_, _, (_, (_, p))) in fields {
                pat(context, p)
            }
        }
        P::At(_, inner) => pat(context, inner),
        P::Or(lhs, rhs) => {
            pat(context, lhs);
            pat(context, rhs);
        }
        P::Wildcard | P::ErrorPat | P::Binder(_, _) | P::Literal(_) => (),
    }
}

//**************************************************************************************************
// Synthesis
//**************************************************************************************************

fn synthesize_getters(mident: ModuleIdent, mdef: &mut T::ModuleDefinition, uses: CrossModuleUses) {
    let CrossModuleUses { constants, users } = uses;
    // The getters are `public(package)`, which compiles to friend visibility, so each using
    // module must be a friend of the defining module. This mirrors what typing does for calls
    // of user-written `public(package)` functions.
    for (user, loc) in users {
        if !mdef.friends.contains_key(&user) {
            let friend = Friend {
                attributes: UniqueMap::new(),
                attr_locs: vec![],
                loc,
            };
            mdef.friends.add(user, friend).unwrap();
        }
    }
    let next_index = mdef
        .functions
        .iter()
        .map(|(_, _, fdef)| fdef.index + 1)
        .max()
        .unwrap_or(0);
    for (i, cname) in constants.into_iter().enumerate() {
        // The name is guaranteed unique: user-defined module members cannot start with '_', and
        // constant names are unique within the module. It is a valid bytecode identifier as long
        // as the constant name is one.
        let getter_symbol = Symbol::from(format!("_const_{}", cname));
        let cdef = mdef.constants.get_mut(&cname).unwrap();
        let loc = cdef.loc;
        let fname = FunctionName(sp(loc, getter_symbol));
        cdef.getter_name = Some(fname);
        let signature = N::FunctionSignature {
            type_parameters: vec![],
            parameters: vec![],
            return_type: cdef.signature.clone(),
        };
        let body_exp = T::exp(
            cdef.signature.clone(),
            sp(loc, T::UnannotatedExp_::Constant(mident, cname)),
        );
        let seq_items = VecDeque::from([sp(loc, T::SequenceItem_::Seq(Box::new(body_exp)))]);
        let body = sp(loc, T::FunctionBody_::Defined((UseFuns::new(0), seq_items)));
        let fdef = T::Function {
            doc: DocComment::empty(),
            warning_filter: filter::empty_filter_scope(),
            index: next_index + i,
            attributes: UniqueMap::new(),
            loc,
            visibility: Visibility::Package(loc),
            compiled_visibility: Visibility::Package(loc),
            entry: None,
            macro_: None,
            signature,
            body,
        };
        mdef.functions
            .add(fname, fdef)
            .expect("ICE getter name should be unique in the module");
    }
}
