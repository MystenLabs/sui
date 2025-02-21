// Copyright (c) The Diem Core Contributors
// Copyright (c) The Move Contributors
// SPDX-License-Identifier: Apache-2.0

use crate::{
    diag,
    diagnostics::Diagnostic,
    expansion::ast::ModuleIdent,
    naming::ast::{self as N, TypeName_},
    parser::ast::DatatypeName,
    shared::{unique_map::UniqueMap, *},
    typing::ast as T,
};
use move_ir_types::location::*;
use petgraph::{algo::tarjan_scc as petgraph_scc, graphmap::DiGraphMap};
use std::collections::BTreeMap;

struct Context {
    datatype_neighbors: BTreeMap<DatatypeName, BTreeMap<DatatypeName, Loc>>,
    current_module: ModuleIdent,
    current_datatype: Option<DatatypeName>,
}

impl Context {
    fn new(current_module: ModuleIdent) -> Self {
        Context {
            current_module,
            datatype_neighbors: BTreeMap::new(),
            current_datatype: None,
        }
    }

    fn add_usage(&mut self, loc: Loc, module: &ModuleIdent, sname: &DatatypeName) {
        if &self.current_module != module {
            return;
        }
        self.datatype_neighbors
            .entry(self.current_datatype.unwrap())
            .or_default()
            .insert(*sname, loc);
    }

    fn datatype_graph(&self) -> DiGraphMap<&DatatypeName, ()> {
        let edges = self
            .datatype_neighbors
            .iter()
            .flat_map(|(parent, children)| children.iter().map(move |(child, _)| (parent, child)));
        DiGraphMap::from_edges(edges)
    }
}

//**************************************************************************************************
// Modules
//**************************************************************************************************

pub fn modules(
    compilation_env: &CompilationEnv,
    modules: &UniqueMap<ModuleIdent, T::ModuleDefinition>,
) {
    modules
        .key_cloned_iter()
        .for_each(|(mname, m)| module(compilation_env, mname, m))
}

fn module(compilation_env: &CompilationEnv, mname: ModuleIdent, module: &T::ModuleDefinition) {
    let reporter = compilation_env.diagnostic_reporter_at_top_level();
    let context = &mut Context::new(mname);
    module
        .structs
        .key_cloned_iter()
        .for_each(|(sname, sdef)| struct_def(context, sname, sdef));
    module
        .enums
        .key_cloned_iter()
        .for_each(|(ename, edef)| enum_def(context, ename, edef));
    let graph = context.datatype_graph();
    // - get the strongly connected components
    // - filter out single nodes that do not connect to themselves
    // - report those cycles
    petgraph_scc(&graph)
        .into_iter()
        .filter(|scc| scc.len() > 1 || graph.contains_edge(scc[0], scc[0]))
        .for_each(|scc| reporter.add_diag(cycle_error(context, &graph, scc[0])))
}

fn struct_def(context: &mut Context, sname: DatatypeName, sdef: &N::StructDefinition) {
    assert!(
        context.current_datatype.is_none(),
        "ICE datatype name not unset"
    );
    context.current_datatype = Some(sname);
    match &sdef.fields {
        N::StructFields::Native(_) => (),
        N::StructFields::Defined(_, fields) => fields
            .iter()
            .for_each(|(_, _, (_, (_, ty)))| type_(context, ty)),
    };
    context.current_datatype = None;
}

fn enum_def(context: &mut Context, ename: DatatypeName, edef: &N::EnumDefinition) {
    assert!(
        context.current_datatype.is_none(),
        "ICE datatype name not unset"
    );
    context.current_datatype = Some(ename);
    for (_, _, vdef) in &edef.variants {
        match &vdef.fields {
            N::VariantFields::Empty => (),
            N::VariantFields::Defined(_, fields) => fields
                .iter()
                .for_each(|(_, _, (_, (_, ty)))| type_(context, ty)),
        }
    }
    context.current_datatype = None;
}

fn type_(context: &mut Context, sp!(loc, ty_): &N::Type) {
    use N::Type_::*;
    match ty_ {
        Var(_) => panic!("ICE tvar in struct field type"),
        Unit | Anything | UnresolvedError | Param(_) => (),
        Ref(_, t) => type_(context, t),
        Apply(_, sp!(_, tn_), tys) => {
            if let TypeName_::ModuleType(m, s) = tn_ {
                context.add_usage(*loc, m, s)
            }
            tys.iter().for_each(|t| type_(context, t))
        }
        Fun(ts, t) => {
            ts.iter().for_each(|t| type_(context, t));
            type_(context, t)
        }
    }
}

fn cycle_error(
    context: &Context,
    graph: &DiGraphMap<&DatatypeName, ()>,
    cycle_node: &DatatypeName,
) -> Diagnostic {
    let cycle = shortest_cycle(graph, cycle_node);

    // For printing uses, sort the cycle by location (earliest first)
    let cycle_strings = cycle
        .iter()
        .map(|m| format!("'{}'", m))
        .collect::<Vec<_>>()
        .join(" contains ");

    let (used_loc, user, used) = best_cycle_loc(context, cycle);

    let use_msg = format!("Invalid field containing '{}' in struct '{}'.", used, user);
    let cycle_msg = format!("Using this struct creates a cycle: {}", cycle_strings);
    diag!(
        TypeSafety::CyclicData,
        (used_loc, use_msg),
        (used_loc, cycle_msg)
    )
}

fn best_cycle_loc<'a>(
    context: &'a Context,
    cycle: Vec<&'a DatatypeName>,
) -> (Loc, &'a DatatypeName, &'a DatatypeName) {
    let get_loc = |user, used| context.datatype_neighbors[user][used];
    let len = cycle.len();
    match len {
        1 => (get_loc(cycle[0], cycle[0]), cycle[0], cycle[0]),
        2 => (get_loc(cycle[0], cycle[1]), cycle[0], cycle[1]),
        _ => {
            let first = cycle[0];
            let user = cycle[len - 2];
            let used = cycle[len - 1];
            assert!(first == used);
            let used_loc = get_loc(user, used);
            (used_loc, user, used)
        }
    }
}
