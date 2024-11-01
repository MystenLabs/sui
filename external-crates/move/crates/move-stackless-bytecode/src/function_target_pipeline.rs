// Copyright (c) The Diem Core Contributors
// Copyright (c) The Move Contributors
// SPDX-License-Identifier: Apache-2.0

use bimap::btree::BiBTreeMap;
use core::fmt;
use std::{
    collections::{btree_map::Entry as MapEntry, BTreeMap, BTreeSet},
    fmt::Formatter,
    fs,
};

use itertools::{Either, Itertools};
use log::{debug, info};
use petgraph::graph::{DiGraph, NodeIndex};

use move_compiler::expansion::ast::{AttributeName_, Attribute_};
use move_compiler::shared::known_attributes::{ExternalAttribute, KnownAttribute::External};
use move_symbol_pool::Symbol;

use move_model::model::{DatatypeId, FunId, FunctionEnv, GlobalEnv, QualifiedId};

use crate::{
    function_target::{FunctionData, FunctionTarget},
    print_targets_for_test,
    stackless_bytecode_generator::StacklessBytecodeGenerator,
    stackless_control_flow_graph::generate_cfg_in_dot_format,
};

/// A data structure which holds data for multiple function targets, and allows to
/// manipulate them as part of a transformation pipeline.
#[derive(Debug)]
pub struct FunctionTargetsHolder {
    targets: BTreeMap<QualifiedId<FunId>, BTreeMap<FunctionVariant, FunctionData>>,
    opaque_specs: BiBTreeMap<QualifiedId<FunId>, QualifiedId<FunId>>,
    no_verify_specs: BTreeSet<QualifiedId<FunId>>,
    no_asserts: BTreeSet<QualifiedId<FunId>>,
    scenario_specs: BTreeSet<QualifiedId<FunId>>,
    datatype_invs: BiBTreeMap<QualifiedId<DatatypeId>, QualifiedId<FunId>>,
}

/// Describes a function verification flavor.
#[derive(Debug, Clone, PartialEq, Eq, PartialOrd, Ord)]
pub enum VerificationFlavor {
    Regular,
    Instantiated(usize),
    Inconsistency(Box<VerificationFlavor>),
}

impl std::fmt::Display for VerificationFlavor {
    fn fmt(&self, f: &mut Formatter<'_>) -> std::fmt::Result {
        match self {
            VerificationFlavor::Regular => write!(f, ""),
            VerificationFlavor::Instantiated(index) => {
                write!(f, "instantiated_{}", index)
            }
            VerificationFlavor::Inconsistency(flavor) => write!(f, "inconsistency_{}", flavor),
        }
    }
}

/// Describes a function target variant.
#[derive(Debug, Clone, PartialEq, Eq, PartialOrd, Ord)]
pub enum FunctionVariant {
    /// The baseline variant which was created from the original Move bytecode and is then
    /// subject of multiple transformations.
    Baseline,
    /// A variant which is instrumented for verification. Only functions which are target
    /// of verification have one of those. There can be multiple verification variants,
    /// each identified by a unique flavor.
    Verification(VerificationFlavor),
}

impl FunctionVariant {
    pub fn is_verified(&self) -> bool {
        matches!(self, FunctionVariant::Verification(..))
    }
}

impl std::fmt::Display for FunctionVariant {
    fn fmt(&self, f: &mut Formatter<'_>) -> std::fmt::Result {
        use FunctionVariant::*;
        match self {
            Baseline => write!(f, "baseline"),
            Verification(VerificationFlavor::Regular) => write!(f, "verification"),
            Verification(v) => write!(f, "verification[{}]", v),
        }
    }
}

/// A trait describing a function target processor.
pub trait FunctionTargetProcessor {
    /// Processes a function variant. Takes as parameter a target holder which can be mutated, the
    /// env of the function being processed, and the target data. During the time the processor is
    /// called, the target data is removed from the holder, and added back once transformation
    /// has finished. This allows the processor to take ownership on the target data.
    fn process(
        &self,
        _targets: &mut FunctionTargetsHolder,
        _fun_env: &FunctionEnv,
        _data: FunctionData,
        _scc_opt: Option<&[FunctionEnv]>,
    ) -> FunctionData {
        unimplemented!()
    }

    /// Same as `process` but can return None to indicate that the function variant is
    /// removed. By default, this maps to `Some(self.process(..))`. One needs to implement
    /// either this function or `process`.
    fn process_and_maybe_remove(
        &self,
        targets: &mut FunctionTargetsHolder,
        func_env: &FunctionEnv,
        data: FunctionData,
        scc_opt: Option<&[FunctionEnv]>,
    ) -> Option<FunctionData> {
        Some(self.process(targets, func_env, data, scc_opt))
    }

    /// Returns a name for this processor. This should be suitable as a file suffix.
    fn name(&self) -> String;

    /// A function which is called once before any `process` call is issued.
    fn initialize(&self, _env: &GlobalEnv, _targets: &mut FunctionTargetsHolder) {}

    /// A function which is called once after the last `process` call.
    fn finalize(&self, _env: &GlobalEnv, _targets: &mut FunctionTargetsHolder) {}

    /// A function which can be implemented to indicate that instead of a sequence of initialize,
    /// process, and finalize, this processor has a single `run` function for the analysis of the
    /// whole set of functions.
    fn is_single_run(&self) -> bool {
        false
    }

    /// To be implemented if `is_single_run()` is true.
    fn run(&self, _env: &GlobalEnv, _targets: &mut FunctionTargetsHolder) {
        unimplemented!()
    }

    /// A function which creates a dump of the processors results, for debugging.
    fn dump_result(
        &self,
        _f: &mut Formatter<'_>,
        _env: &GlobalEnv,
        _targets: &FunctionTargetsHolder,
    ) -> fmt::Result {
        Ok(())
    }
}

pub struct ProcessorResultDisplay<'a> {
    pub env: &'a GlobalEnv,
    pub targets: &'a FunctionTargetsHolder,
    pub processor: &'a dyn FunctionTargetProcessor,
}

impl<'a> fmt::Display for ProcessorResultDisplay<'a> {
    fn fmt(&self, f: &mut Formatter<'_>) -> fmt::Result {
        self.processor.dump_result(f, self.env, self.targets)
    }
}

/// A processing pipeline for function targets.
#[derive(Default)]
pub struct FunctionTargetPipeline {
    processors: Vec<Box<dyn FunctionTargetProcessor>>,
}

impl FunctionTargetsHolder {
    pub fn new() -> Self {
        Self {
            targets: BTreeMap::new(),
            opaque_specs: BiBTreeMap::new(),
            no_verify_specs: BTreeSet::new(),
            no_asserts: BTreeSet::new(),
            scenario_specs: BTreeSet::new(),
            datatype_invs: BiBTreeMap::new(),
        }
    }

    /// Get an iterator for all functions this holder.
    pub fn get_funs(&self) -> impl Iterator<Item = QualifiedId<FunId>> + '_ {
        self.targets.keys().cloned()
    }

    /// Gets an iterator for all functions and variants in this holder.
    pub fn get_funs_and_variants(
        &self,
    ) -> impl Iterator<Item = (QualifiedId<FunId>, FunctionVariant)> + '_ {
        self.targets
            .iter()
            .flat_map(|(id, vs)| vs.keys().map(move |v| (*id, v.clone())))
    }

    pub fn opaque_specs(&self) -> &BiBTreeMap<QualifiedId<FunId>, QualifiedId<FunId>> {
        &self.opaque_specs
    }

    pub fn get_fun_by_opaque_spec(&self, id: &QualifiedId<FunId>) -> Option<&QualifiedId<FunId>> {
        self.opaque_specs.get_by_left(id)
    }

    pub fn get_opaque_spec_by_fun(&self, id: &QualifiedId<FunId>) -> Option<&QualifiedId<FunId>> {
        self.opaque_specs.get_by_right(id)
    }

    pub fn no_verify_specs(&self) -> &BTreeSet<QualifiedId<FunId>> {
        &self.no_verify_specs
    }

    pub fn no_asserts(&self) -> &BTreeSet<QualifiedId<FunId>> {
        &self.no_asserts
    }

    pub fn scenario_specs(&self) -> &BTreeSet<QualifiedId<FunId>> {
        &self.scenario_specs
    }

    pub fn is_spec(&self, id: &QualifiedId<FunId>) -> bool {
        self.get_fun_by_opaque_spec(id).is_some() || self.scenario_specs.contains(id)
    }

    pub fn is_verified_spec(&self, id: &QualifiedId<FunId>) -> bool {
        self.is_spec(id) && !self.no_verify_specs.contains(id)
    }

    pub fn specs(&self) -> impl Iterator<Item = &QualifiedId<FunId>> {
        self.opaque_specs
            .left_values()
            .chain(self.scenario_specs.iter())
    }

    pub fn has_no_verify_spec(&self, id: &QualifiedId<FunId>) -> bool {
        match self.get_opaque_spec_by_fun(id) {
            Some(spec_id) => self.no_verify_specs.contains(spec_id),
            None => false,
        }
    }

    pub fn get_inv_by_datatype(&self, id: &QualifiedId<DatatypeId>) -> Option<&QualifiedId<FunId>> {
        self.datatype_invs.get_by_left(id)
    }

    pub fn get_datatype_by_inv(&self, id: &QualifiedId<FunId>) -> Option<&QualifiedId<DatatypeId>> {
        self.datatype_invs.get_by_right(id)
    }

    /// Adds a new function target. The target will be initialized from the Move byte code.
    pub fn add_target(&mut self, func_env: &FunctionEnv<'_>) {
        let generator = StacklessBytecodeGenerator::new(func_env);
        let data = generator.generate_function();
        self.targets
            .entry(func_env.get_qualified_id())
            .or_default()
            .insert(FunctionVariant::Baseline, data);
        func_env.get_name_str().strip_suffix("_spec").map(|name| {
            if let Some(ext_attr) = func_env
                .get_toplevel_attributes()
                .get_(&External(ExternalAttribute))
            {
                match &ext_attr.value {
                    Attribute_::Parameterized(_, inner_attrs) => {
                        if inner_attrs
                            .contains_key_(&AttributeName_::Unknown(Symbol::from("no_verify")))
                        {
                            self.no_verify_specs.insert(func_env.get_qualified_id());
                        }
                        if inner_attrs
                            .contains_key_(&AttributeName_::Unknown(Symbol::from("no_asserts")))
                        {
                            self.no_asserts.insert(func_env.get_qualified_id());
                        }
                    }
                    _ => {}
                }
            }

            match func_env
                .module_env
                .find_function(func_env.symbol_pool().make(name))
            {
                Some(target_func_env) => {
                    self.opaque_specs.insert(
                        func_env.get_qualified_id(),
                        target_func_env.get_qualified_id(),
                    );
                }
                None => {
                    self.scenario_specs.insert(func_env.get_qualified_id());
                }
            }
        });

        func_env.get_name_str().strip_suffix("_inv").map(|name| {
            if let Some(struct_env) = func_env
                .module_env
                .find_struct(func_env.symbol_pool().make(name))
            {
                self.datatype_invs
                    .insert(struct_env.get_qualified_id(), func_env.get_qualified_id());
            }
        });
    }

    /// Gets a function target for read-only consumption, for the given variant.
    pub fn get_target<'env>(
        &'env self,
        func_env: &'env FunctionEnv<'env>,
        variant: &FunctionVariant,
    ) -> FunctionTarget<'env> {
        let data = self
            .get_data(&func_env.get_qualified_id(), variant)
            .unwrap_or_else(|| {
                panic!(
                    "expected function target: {} ({:?})",
                    func_env.get_full_name_str(),
                    variant
                )
            });
        FunctionTarget::new(func_env, data)
    }

    pub fn has_target(&self, func_env: &FunctionEnv<'_>, variant: &FunctionVariant) -> bool {
        self.get_data(&func_env.get_qualified_id(), variant)
            .is_some()
    }

    /// Gets all available variants for function.
    pub fn get_target_variants(&self, func_env: &FunctionEnv<'_>) -> Vec<FunctionVariant> {
        self.targets
            .get(&func_env.get_qualified_id())
            .expect("function targets exist")
            .keys()
            .cloned()
            .collect_vec()
    }

    /// Gets targets for all available variants.
    pub fn get_targets<'env>(
        &'env self,
        func_env: &'env FunctionEnv<'env>,
    ) -> Vec<(FunctionVariant, FunctionTarget<'env>)> {
        self.targets
            .get(&func_env.get_qualified_id())
            .expect("function targets exist")
            .iter()
            .map(|(v, d)| (v.clone(), FunctionTarget::new(func_env, d)))
            .collect_vec()
    }

    /// Gets function data for a variant.
    pub fn get_data(
        &self,
        id: &QualifiedId<FunId>,
        variant: &FunctionVariant,
    ) -> Option<&FunctionData> {
        self.targets.get(id).and_then(|vs| vs.get(variant))
    }

    /// Gets mutable function data for a variant.
    pub fn get_data_mut(
        &mut self,
        id: &QualifiedId<FunId>,
        variant: &FunctionVariant,
    ) -> Option<&mut FunctionData> {
        self.targets.get_mut(id).and_then(|vs| vs.get_mut(variant))
    }

    /// Removes function data for a variant.
    pub fn remove_target_data(
        &mut self,
        id: &QualifiedId<FunId>,
        variant: &FunctionVariant,
    ) -> FunctionData {
        self.targets
            .get_mut(id)
            .expect("function target exists")
            .remove(variant)
            .expect("variant exists")
    }

    /// Sets function data for a function's variant.
    pub fn insert_target_data(
        &mut self,
        id: &QualifiedId<FunId>,
        variant: FunctionVariant,
        data: FunctionData,
    ) {
        self.targets
            .get_mut(id)
            .expect(&format!(
                "function qualified id {:#?} not found in targets",
                id
            ))
            .insert(variant, data);
    }

    /// Processes the function target data for given function.
    fn process(
        &mut self,
        func_env: &FunctionEnv,
        processor: &dyn FunctionTargetProcessor,
        scc_opt: Option<&[FunctionEnv]>,
    ) {
        let id = func_env.get_qualified_id();
        for variant in self.get_target_variants(func_env) {
            // Remove data so we can own it.
            let data = self.remove_target_data(&id, &variant);
            if let Some(processed_data) =
                processor.process_and_maybe_remove(self, func_env, data, scc_opt)
            {
                // Put back processed data.
                self.insert_target_data(&id, variant, processed_data);
            }
        }
    }

    pub fn dump_spec_info(&self, env: &GlobalEnv, f: &mut fmt::Formatter) -> fmt::Result {
        writeln!(f, "=== function target holder ===")?;
        writeln!(f)?;
        writeln!(f, "Verification specs:")?;
        for spec in self.specs() {
            let fun_env = env.get_function(*spec);
            if self.is_verified_spec(spec)
                && self.has_target(
                    &fun_env,
                    &FunctionVariant::Verification(VerificationFlavor::Regular),
                )
            {
                writeln!(f, "  {}", fun_env.get_full_name_str())?;
            }
        }
        writeln!(f, "Opaque specs:")?;
        for (spec, fun) in self.opaque_specs.iter() {
            writeln!(
                f,
                "  {} -> {}",
                env.get_function(*spec).get_full_name_str(),
                env.get_function(*fun).get_full_name_str()
            )?;
        }
        writeln!(f, "No verify specs:")?;
        for spec in self.no_verify_specs.iter() {
            writeln!(f, "  {}", env.get_function(*spec).get_full_name_str())?;
        }
        writeln!(f, "No asserts specs:")?;
        for spec in self.no_asserts.iter() {
            writeln!(f, "  {}", env.get_function(*spec).get_full_name_str())?;
        }
        writeln!(f, "Scenario specs:")?;
        for spec in self.scenario_specs.iter() {
            writeln!(f, "  {}", env.get_function(*spec).get_full_name_str())?;
        }
        writeln!(f, "Datatype invariants:")?;
        for (datatype, inv) in self.datatype_invs.iter() {
            writeln!(
                f,
                "  {} -> {}",
                env.get_struct(*datatype).get_full_name_str(),
                env.get_function(*inv).get_full_name_str(),
            )?;
        }
        Ok(())
    }
}

pub struct FunctionTargetsHolderDisplay<'a> {
    pub targets: &'a FunctionTargetsHolder,
    pub env: &'a GlobalEnv,
}

impl<'a> fmt::Display for FunctionTargetsHolderDisplay<'a> {
    fn fmt(&self, f: &mut Formatter<'_>) -> fmt::Result {
        self.targets.dump_spec_info(self.env, f)
    }
}

impl FunctionTargetPipeline {
    /// Adds a processor to this pipeline. Processor will be called in the order they have been
    /// added.
    pub fn add_processor(&mut self, processor: Box<dyn FunctionTargetProcessor>) {
        self.processors.push(processor)
    }

    /// Gets the last processor in the pipeline, for testing.
    pub fn last_processor(&self) -> &dyn FunctionTargetProcessor {
        self.processors
            .iter()
            .last()
            .expect("pipeline not empty")
            .as_ref()
    }

    /// Build the call graph
    fn build_call_graph(
        env: &GlobalEnv,
        targets: &FunctionTargetsHolder,
    ) -> (
        DiGraph<QualifiedId<FunId>, ()>,
        BTreeMap<QualifiedId<FunId>, NodeIndex>,
    ) {
        let mut graph = DiGraph::new();
        let mut nodes = BTreeMap::new();
        for fun_id in targets.get_funs() {
            let node_idx = graph.add_node(fun_id);
            nodes.insert(fun_id, node_idx);
        }
        for fun_id in targets.get_funs() {
            let src_idx = nodes.get(&fun_id).unwrap();
            let fun_env = env.get_function(fun_id);
            for callee in fun_env.get_called_functions() {
                let dst_idx = nodes
                    .get(&callee)
                    .expect("callee is not in function targets");
                graph.add_edge(*src_idx, *dst_idx, ());
            }
        }
        (graph, nodes)
    }

    /// Collect strongly connected components (SCCs) from the call graph.
    /// Returns a list of node SCCs in reverse topological order, and a map from function id
    /// to other functions in the same SCC.
    fn derive_call_graph_sccs(
        env: &GlobalEnv,
        graph: &DiGraph<QualifiedId<FunId>, ()>,
    ) -> (
        Vec<Vec<NodeIndex>>,
        BTreeMap<QualifiedId<FunId>, Option<BTreeSet<QualifiedId<FunId>>>>,
    ) {
        let mut sccs = BTreeMap::new();
        // Returned SCCs are in reverse topological order.
        let scc_nodes = petgraph::algo::tarjan_scc(graph);
        for scc in scc_nodes.clone() {
            let mut part = BTreeSet::new();
            let mut is_cyclic = scc.len() > 1;
            for node_idx in scc {
                let fun_id = *graph.node_weight(node_idx).unwrap();
                let fun_env = env.get_function(fun_id);
                if !is_cyclic && fun_env.get_called_functions().contains(&fun_id) {
                    is_cyclic = true;
                }
                let inserted = part.insert(fun_id);
                assert!(inserted);
            }

            if is_cyclic {
                for fun_id in &part {
                    let existing = sccs.insert(*fun_id, Some(part.clone()));
                    assert!(existing.is_none());
                }
            } else {
                let fun_id = part.into_iter().next().unwrap();
                let existing = sccs.insert(fun_id, None);
                assert!(existing.is_none());
            }
        }
        (scc_nodes, sccs)
    }

    /// Sort the call graph in topological order with strongly connected components (SCCs)
    /// to represent recursive calls.
    pub fn sort_targets_in_topological_order(
        env: &GlobalEnv,
        targets: &FunctionTargetsHolder,
    ) -> Vec<Either<QualifiedId<FunId>, Vec<QualifiedId<FunId>>>> {
        // collect sccs
        let (graph, _nodes) = Self::build_call_graph(env, targets);
        let (scc_nodes, scc_map) = Self::derive_call_graph_sccs(env, &graph);

        let mut scc_staging = BTreeMap::new();
        for scc_opt in scc_map.values() {
            match scc_opt.as_ref() {
                None => (),
                Some(scc) => {
                    scc_staging.insert(scc, vec![]);
                }
            }
        }

        // Construct the work list from a deterministic topological ordering.
        let mut worklist = vec![];
        for scc in scc_nodes.into_iter().rev() {
            for node_idx in scc {
                let fun_id = *graph.node_weight(node_idx).unwrap();
                let fun_env = env.get_function(fun_id);
                worklist.push((
                    fun_id,
                    fun_env.get_called_functions().into_iter().collect_vec(),
                ));
            }
        }

        // analyze bottom-up from the leaves of the call graph
        // NOTE: this algorithm produces a deterministic ordering of functions to be analyzed
        let mut dep_ordered = vec![];
        while let Some((call_id, callees)) = worklist.pop() {
            // At this point, one of two things is true:
            // 1. callees is empty (common case)
            // 2. callees is nonempty and call_id is part of a recursive or mutually recursive function group

            match scc_map.get(&call_id).unwrap().as_ref() {
                None => {
                    // case 1: non-recursive call
                    assert!(callees.is_empty());
                    dep_ordered.push(Either::Left(call_id));
                }
                Some(scc) => {
                    // case 2: recursive call group
                    match scc_staging.entry(scc) {
                        MapEntry::Vacant(_) => {
                            panic!("all scc groups should be in staging")
                        }
                        MapEntry::Occupied(mut entry) => {
                            let scc_vec = entry.get_mut();
                            scc_vec.push(call_id);
                            if scc_vec.len() == scc.len() {
                                dep_ordered.push(Either::Right(entry.remove()));
                            }
                        }
                    }
                }
            }

            // update the worklist
            for (_, callees) in worklist.iter_mut() {
                callees.retain(|e| e != &call_id);
            }
        }

        // ensure that everything is cleared
        assert!(scc_staging.is_empty());

        // return the ordered dep list
        dep_ordered
    }

    /// Runs the pipeline on all functions in the targets holder. Processors are run on each
    /// individual function in breadth-first fashion; i.e. a processor can expect that processors
    /// preceding it in the pipeline have been executed for all functions before it is called.
    pub fn run_with_hook<H1, H2>(
        &self,
        env: &GlobalEnv,
        targets: &mut FunctionTargetsHolder,
        hook_before_pipeline: H1,
        hook_after_each_processor: H2,
    ) where
        H1: Fn(&FunctionTargetsHolder),
        H2: Fn(usize, &dyn FunctionTargetProcessor, &FunctionTargetsHolder),
    {
        let topological_order = Self::sort_targets_in_topological_order(env, targets);
        hook_before_pipeline(targets);
        for (step_count, processor) in self.processors.iter().enumerate() {
            if processor.is_single_run() {
                processor.run(env, targets);
            } else {
                processor.initialize(env, targets);
                for item in &topological_order {
                    match item {
                        Either::Left(fid) => {
                            let func_env = env.get_function(*fid);
                            targets.process(&func_env, processor.as_ref(), None);
                        }
                        Either::Right(scc) => 'fixedpoint: loop {
                            let scc_env: Vec<_> =
                                scc.iter().map(|fid| env.get_function(*fid)).collect();
                            for fid in scc {
                                let func_env = env.get_function(*fid);
                                targets.process(&func_env, processor.as_ref(), Some(&scc_env));
                            }

                            // check for fixedpoint in summaries
                            for fid in scc {
                                let func_env = env.get_function(*fid);
                                for (_, target) in targets.get_targets(&func_env) {
                                    if !target.data.annotations.reached_fixedpoint() {
                                        continue 'fixedpoint;
                                    }
                                }
                            }
                            // fixedpoint reached when execution hits this line
                            break 'fixedpoint;
                        },
                    }
                }
                processor.finalize(env, targets);
            }
            hook_after_each_processor(step_count + 1, processor.as_ref(), targets);
        }
    }

    /// Run the pipeline on all functions in the targets holder, with no hooks in effect
    pub fn run(&self, env: &GlobalEnv, targets: &mut FunctionTargetsHolder) {
        self.run_with_hook(env, targets, |_| {}, |_, _, _| {})
    }

    /// Runs the pipeline on all functions in the targets holder, dump the bytecode before the
    /// pipeline as well as after each processor pass. If `dump_cfg` is set, dump the per-function
    /// control-flow graph (in dot format) too.
    pub fn run_with_dump(
        &self,
        env: &GlobalEnv,
        targets: &mut FunctionTargetsHolder,
        dump_base_name: &str,
        dump_cfg: bool,
    ) {
        self.run_with_hook(
            env,
            targets,
            |holders| {
                Self::dump_to_file(
                    dump_base_name,
                    0,
                    "stackless",
                    &Self::get_pre_pipeline_dump(env, holders),
                )
            },
            |step_count, processor, holders| {
                let suffix = processor.name();
                Self::dump_to_file(
                    dump_base_name,
                    step_count,
                    &suffix,
                    &Self::get_per_processor_dump(env, holders, processor),
                );
                if dump_cfg {
                    Self::dump_cfg(env, holders, dump_base_name, step_count, &suffix);
                }
            },
        );
    }

    fn print_targets(env: &GlobalEnv, name: &str, targets: &FunctionTargetsHolder) -> String {
        print_targets_for_test(env, &format!("after processor `{}`", name), targets)
    }

    fn get_pre_pipeline_dump(env: &GlobalEnv, targets: &FunctionTargetsHolder) -> String {
        Self::print_targets(env, "stackless", targets)
    }

    fn get_per_processor_dump(
        env: &GlobalEnv,
        targets: &FunctionTargetsHolder,
        processor: &dyn FunctionTargetProcessor,
    ) -> String {
        let mut dump = format!(
            "{}",
            ProcessorResultDisplay {
                env,
                targets,
                processor,
            }
        );
        if !processor.is_single_run() {
            if !dump.is_empty() {
                dump = format!("\n\n{}", dump);
            }
            dump.push_str(&Self::print_targets(env, &processor.name(), targets));
        }
        dump
    }

    fn dump_to_file(base_name: &str, step_count: usize, suffix: &str, content: &str) {
        let dump = format!("{}\n", content.trim());
        let file_name = format!("{}_{}_{}.bytecode", base_name, step_count, suffix);
        debug!("dumping bytecode to `{}`", file_name);
        fs::write(&file_name, dump).expect("dumping bytecode");
    }

    /// Generate dot files for control-flow graphs.
    fn dump_cfg(
        env: &GlobalEnv,
        targets: &FunctionTargetsHolder,
        base_name: &str,
        step_count: usize,
        suffix: &str,
    ) {
        for (fun_id, variants) in &targets.targets {
            let func_env = env.get_function(*fun_id);
            let func_name = func_env.get_full_name_str();
            let func_name = func_name.replace("::", "__");
            for (variant, data) in variants {
                if !data.code.is_empty() {
                    let dot_file = format!(
                        "{}_{}_{}_{}_{}_cfg.dot",
                        base_name, step_count, suffix, func_name, variant
                    );
                    debug!("generating dot graph for cfg in `{}`", dot_file);
                    let func_target = FunctionTarget::new(&func_env, data);
                    let dot_graph = generate_cfg_in_dot_format(&func_target);
                    fs::write(&dot_file, &dot_graph).expect("generating dot file for CFG");
                }
            }
        }
    }
}
