// Copyright (c) The Diem Core Contributors
// Copyright (c) The Move Contributors
// SPDX-License-Identifier: Apache-2.0

use crate::{
    diagnostics::{codes::*, Diagnostic, DiagnosticReporter},
    expansion::ast::{Address, ModuleIdent, Value_},
    ice,
    naming::ast::{self as N, Neighbor, Neighbor_},
    shared::{unique_map::UniqueMap, *},
    typing::ast as T,
};
use move_ir_types::location::*;
use move_proc_macros::growing_stack;
use petgraph::{algo::toposort as petgraph_toposort, graphmap::DiGraphMap};
use std::collections::{BTreeMap, BTreeSet};

//**************************************************************************************************
// Entry
//**************************************************************************************************

pub fn program(
    compilation_env: &CompilationEnv,
    modules: &mut UniqueMap<ModuleIdent, T::ModuleDefinition>,
) {
    let imm_modules = &modules;
    let mut context = Context::new(compilation_env, imm_modules);
    module_defs(&mut context, modules);

    let Context {
        module_neighbors,
        neighbors_by_node,
        addresses_by_node,
        ..
    } = context;
    let graph = dependency_graph(&module_neighbors);
    match petgraph_toposort(&graph, None) {
        Err(cycle_node) => {
            let cycle_ident = *cycle_node.node_id();
            let error = cycle_error(&module_neighbors, cycle_ident);
            context.reporter.add_diag(error);
        }
        Ok(ordered_ids) => {
            for (order, mident) in ordered_ids.iter().rev().enumerate() {
                modules.get_mut(mident).unwrap().dependency_order = order;
            }
        }
    }
    for (node, neighbors) in neighbors_by_node {
        let module = modules.get_mut(&node).unwrap();
        module.immediate_neighbors = neighbors;
    }
    for (node, used_addresses) in addresses_by_node {
        let module = modules.get_mut(&node).unwrap();
        module.used_addresses = used_addresses;
    }
}

#[derive(Clone, Copy, Eq, PartialEq, Ord, PartialOrd)]
enum DepType {
    Use,
    Friend,
}

struct Context<'a, 'env> {
    #[allow(unused)]
    env: &'env CompilationEnv,
    reporter: DiagnosticReporter<'env>,
    modules: &'a UniqueMap<ModuleIdent, T::ModuleDefinition>,
    // A union of uses and friends for modules (used for cyclyc dependency checking)
    // - if A uses B,    add edge A -> B
    // - if A friends B, add edge B -> A
    module_neighbors: BTreeMap<ModuleIdent, BTreeMap<ModuleIdent, BTreeMap<DepType, Loc>>>,
    // A summary of neighbors keyed by module
    neighbors_by_node: BTreeMap<ModuleIdent, UniqueMap<ModuleIdent, Neighbor>>,
    // All addresses used by a node
    addresses_by_node: BTreeMap<ModuleIdent, BTreeSet<Address>>,
    // The module we are currently exploring
    current_node: Option<ModuleIdent>,
}

impl<'a, 'env> Context<'a, 'env> {
    fn new(
        env: &'env CompilationEnv,
        modules: &'a UniqueMap<ModuleIdent, T::ModuleDefinition>,
    ) -> Self {
        let reporter = env.diagnostic_reporter_at_top_level();
        Context {
            env,
            reporter,
            modules,
            module_neighbors: BTreeMap::new(),
            neighbors_by_node: BTreeMap::new(),
            addresses_by_node: BTreeMap::new(),
            current_node: None,
        }
    }

    fn add_neighbor(&mut self, mident: ModuleIdent, dep_type: DepType, loc: Loc) {
        if !self.modules.contains_key(&mident) {
            // as the dependency checking happens before the naming phase, it is possible to refer
            // to a module with a ModuleIdent outside of the compilation context. Do not add such
            // modules as neighbors.
            return;
        }

        let current = self.current_node.unwrap();
        if mident == current {
            // do not add the module itself as a neighbor
            return;
        }

        let neighbor_ = match dep_type {
            DepType::Use => Neighbor_::Dependency,
            DepType::Friend => Neighbor_::Friend,
        };
        let current_neighbors = self.neighbors_by_node.entry(current).or_default();
        let current_used_addresses = self.addresses_by_node.entry(current).or_default();
        current_neighbors.remove(&mident);
        current_neighbors.add(mident, sp(loc, neighbor_)).unwrap();
        current_used_addresses.insert(mident.value.address);

        let (node, new_neighbor) = match dep_type {
            DepType::Use => (current, mident),
            DepType::Friend => (mident, current),
        };
        let m = self
            .module_neighbors
            .entry(node)
            .or_default()
            .entry(new_neighbor)
            .or_default();
        if m.contains_key(&dep_type) {
            return;
        }
        m.insert(dep_type, loc);
    }

    fn add_usage(&mut self, mident: ModuleIdent, loc: Loc) {
        self.add_neighbor(mident, DepType::Use, loc);
    }

    fn add_friend(&mut self, mident: ModuleIdent, loc: Loc) {
        self.add_neighbor(mident, DepType::Friend, loc);
    }

    fn add_address_usage(&mut self, address: Address) {
        self.addresses_by_node
            .entry(self.current_node.unwrap())
            .or_default()
            .insert(address);
    }
}

fn dependency_graph(
    deps: &BTreeMap<ModuleIdent, BTreeMap<ModuleIdent, BTreeMap<DepType, Loc>>>,
) -> DiGraphMap<&ModuleIdent, ()> {
    let mut graph = DiGraphMap::new();
    for (parent, children) in deps {
        if children.is_empty() {
            graph.add_node(parent);
        } else {
            for child in children.keys() {
                graph.add_edge(parent, child, ());
            }
        }
    }
    graph
}

fn cycle_error(
    deps: &BTreeMap<ModuleIdent, BTreeMap<ModuleIdent, BTreeMap<DepType, Loc>>>,
    cycle_ident: ModuleIdent,
) -> Diagnostic {
    let graph = dependency_graph(deps);
    // For printing uses, sort the cycle by location (earliest first)
    let cycle = shortest_cycle(&graph, &cycle_ident);

    let mut cycle_info = cycle
        .windows(2)
        .map(|pair| {
            let node = pair[0];
            let neighbor = pair[1];
            let relations = deps.get(node).unwrap().get(neighbor).unwrap();
            match (
                relations.get(&DepType::Use),
                relations.get(&DepType::Friend),
            ) {
                (Some(loc), _) => (
                    *loc,
                    DepType::Use,
                    format!("'{}' uses '{}'", neighbor, node),
                    node,
                    neighbor,
                ),
                (_, Some(loc)) => (
                    *loc,
                    DepType::Friend,
                    format!("'{}' is a friend of '{}'", node, neighbor),
                    node,
                    neighbor,
                ),
                (None, None) => unreachable!(),
            }
        })
        .collect::<Vec<_>>();
    debug_assert!({
        let first_node = cycle_info.first().unwrap().3;
        let last_neighbor = cycle_info.last().unwrap().4;
        first_node == last_neighbor
    });
    let cycle_last = cycle_info.pop().unwrap();

    let (cycle_loc, use_msg) = {
        let (loc, dep_type, case_msg, _node, _neighbor) = cycle_last;
        let case = match dep_type {
            DepType::Use => "use",
            DepType::Friend => "friend",
        };
        let msg = format!(
            "{}. This '{}' relationship creates a dependency cycle.",
            case_msg, case
        );
        (loc, msg)
    };

    Diagnostic::new(
        Declarations::InvalidModule,
        (cycle_loc, use_msg),
        cycle_info
            .into_iter()
            .map(|(loc, _dep_type, msg, _node, _neighbor)| (loc, msg)),
        std::iter::empty::<String>(),
    )
}

//**************************************************************************************************
// Modules
//**************************************************************************************************

fn module_defs(context: &mut Context, modules: &UniqueMap<ModuleIdent, T::ModuleDefinition>) {
    modules
        .key_cloned_iter()
        .for_each(|(mident, mdef)| module(context, mident, mdef))
}

fn module(context: &mut Context, mident: ModuleIdent, mdef: &T::ModuleDefinition) {
    context.current_node = Some(mident);
    mdef.friends
        .key_cloned_iter()
        .for_each(|(mident, friend)| context.add_friend(mident, friend.loc));
    mdef.structs
        .iter()
        .for_each(|(_, _, sdef)| struct_def(context, sdef));
    mdef.enums
        .iter()
        .for_each(|(_, _, edef)| enum_def(context, edef));
    mdef.functions
        .iter()
        .for_each(|(_, _, fdef)| function(context, fdef));
}

//**************************************************************************************************
// Function
//**************************************************************************************************

fn function(context: &mut Context, fdef: &T::Function) {
    match &fdef.body.value {
        T::FunctionBody_::Defined(seq) => {
            function_signature(context, &fdef.signature);
            sequence(context, seq)
        }
        T::FunctionBody_::Native => function_signature(context, &fdef.signature),
        // macros do not add dependencies
        T::FunctionBody_::Macro => (),
    }
}

fn function_signature(context: &mut Context, sig: &N::FunctionSignature) {
    types(context, sig.parameters.iter().map(|(_, _, st)| st));
    type_(context, &sig.return_type)
}

//**************************************************************************************************
// Data Types
//**************************************************************************************************

fn struct_def(context: &mut Context, sdef: &N::StructDefinition) {
    if let N::StructFields::Defined(_, fields) = &sdef.fields {
        fields
            .iter()
            .for_each(|(_, _, (_, (_, bt)))| type_(context, bt));
    }
}

fn enum_def(context: &mut Context, edef: &N::EnumDefinition) {
    for (_, _, variant) in &edef.variants {
        if let N::VariantFields::Defined(_, fields) = &variant.fields {
            fields
                .iter()
                .for_each(|(_, _, (_, (_, bt)))| type_(context, bt));
        }
    }
}

//**************************************************************************************************
// Types
//**************************************************************************************************

fn types<'a>(context: &mut Context, tys: impl IntoIterator<Item = &'a N::Type>) {
    tys.into_iter().for_each(|ty| type_(context, ty))
}

fn type_(context: &mut Context, sp!(_, ty_): &N::Type) {
    use N::Type_ as T;
    match ty_ {
        T::Apply(_, tn, tys) => {
            type_name(context, tn);
            types(context, tys);
        }
        T::Ref(_, t) => type_(context, t),
        T::Fun(tys, t) => {
            types(context, tys);
            type_(context, t);
        }
        T::Unit | T::Param(_) | T::Var(_) | T::Anything | T::UnresolvedError => (),
    }
}

fn type_name(context: &mut Context, sp!(loc, tn_): &N::TypeName) {
    match tn_ {
        N::TypeName_::Multiple(_) | N::TypeName_::Builtin(_) => (),
        N::TypeName_::ModuleType(m, _) => {
            context.add_usage(*m, *loc);
        }
    }
}

fn type_opt(context: &mut Context, t_opt: &Option<N::Type>) {
    t_opt.iter().for_each(|t| type_(context, t))
}

//**************************************************************************************************
// Expressions
//**************************************************************************************************

fn sequence(context: &mut Context, (_, sequence): &T::Sequence) {
    use T::SequenceItem_ as SI;
    for sp!(_, item_) in sequence {
        match item_ {
            SI::Seq(e) => exp(context, e),
            SI::Declare(sp!(_, lvs_)) => {
                lvalues(context, lvs_);
            }
            SI::Bind(sp!(_, lvs_), ty_opts, e) => {
                lvalues(context, lvs_);
                for ty_opt in ty_opts {
                    type_opt(context, ty_opt);
                }
                exp(context, e)
            }
        }
    }
}

fn lvalues<'a>(context: &mut Context, al: impl IntoIterator<Item = &'a T::LValue>) {
    al.into_iter().for_each(|a| lvalue(context, a))
}

fn lvalue(context: &mut Context, sp!(loc, lv_): &T::LValue) {
    use T::LValue_ as L;
    match lv_ {
        L::Ignore => (),
        L::Var { ty, .. } => type_(context, ty),
        L::Unpack(m, _, tys, fields) | L::BorrowUnpack(_, m, _, tys, fields) => {
            context.add_usage(*m, *loc);
            types(context, tys);
            for (_, _, (_, (_, field))) in fields {
                lvalue(context, field)
            }
        }
        L::BorrowUnpackVariant(..) | L::UnpackVariant(..) => {
            context.reporter.add_diag(ice!((
                *loc,
                "variant unpacking shouldn't occur before match expansion"
            )));
        }
    }
}

#[growing_stack]
fn exp(context: &mut Context, e: &T::Exp) {
    use T::UnannotatedExp_ as E;
    match &e.exp.value {
        E::Value(sp!(_, Value_::Address(a))) => context.add_address_usage(*a),

        E::ModuleCall(c) => {
            let T::ModuleCall {
                module,
                type_arguments,
                arguments,
                ..
            } = &**c;
            context.add_usage(*module, e.exp.loc);
            types(context, type_arguments);
            exp(context, arguments);
        }
        E::Builtin(_, e) => exp(context, e),
        E::Vector(_, _, ty, e) => {
            type_(context, ty);
            exp(context, e);
        }
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
        E::VariantMatch(..) => {
            context.reporter.add_diag(ice!((
                e.exp.loc,
                "shouldn't find variant match before HLIR lowering"
            )));
        }
        E::While(_, e1, e2) => {
            exp(context, e1);
            exp(context, e2);
        }
        E::Loop { body, .. } => exp(context, body),
        E::NamedBlock(_, seq) => sequence(context, seq),
        E::Block(seq) => sequence(context, seq),
        E::Assign(sp!(_, lvs_), ty_opts, e) => {
            lvalues(context, lvs_);
            for ty_opt in ty_opts {
                type_opt(context, ty_opt);
            }
            exp(context, e)
        }
        E::Mutate(e1, e2) => {
            exp(context, e1);
            exp(context, e2);
        }
        E::Return(e) => exp(context, e),
        E::Abort(e) => exp(context, e),
        E::Give(_, e) => exp(context, e),
        E::Dereference(e) => exp(context, e),
        E::UnaryExp(_, e) => exp(context, e),
        E::BinopExp(e1, _, _, e2) => {
            exp(context, e1);
            exp(context, e2);
        }
        E::Pack(m, _, tys, fields) => {
            context.add_usage(*m, e.exp.loc);
            types(context, tys);
            for (_, _, (_, (_, e))) in fields {
                exp(context, e)
            }
        }
        E::PackVariant(m, _, _, tys, fields) => {
            context.add_usage(*m, e.exp.loc);
            types(context, tys);
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
        E::Borrow(_, e, _) => exp(context, e),
        E::TempBorrow(_, e) => exp(context, e),
        E::Cast(e, ty) => {
            exp(context, e);
            type_(context, ty)
        }
        E::Annotate(e, ty) => {
            exp(context, e);
            type_(context, ty)
        }
        E::Unit { .. }
        | E::Value(_)
        | E::Move { .. }
        | E::Copy { .. }
        | E::Use(_)
        | E::Constant(..)
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
        P::Variant(m, _, _, tys, fields)
        | P::BorrowVariant(_, m, _, _, tys, fields)
        | P::Struct(m, _, tys, fields)
        | P::BorrowStruct(_, m, _, tys, fields) => {
            context.add_usage(*m, p.pat.loc);
            types(context, tys);
            for (_, _, (_, (_, p))) in fields {
                pat(context, p)
            }
        }
        P::At(_, inner) => pat(context, inner),
        P::Or(lhs, rhs) => {
            pat(context, lhs);
            pat(context, rhs);
        }
        P::Constant(_, _) | P::Wildcard | P::ErrorPat | P::Binder(_, _) | P::Literal(_) => (),
    }
}
