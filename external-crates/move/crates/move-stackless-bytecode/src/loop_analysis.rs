// Copyright (c) The Diem Core Contributors
// Copyright (c) The Move Contributors
// SPDX-License-Identifier: Apache-2.0

use std::collections::{BTreeMap, BTreeSet};

use move_binary_format::file_format::CodeOffset;
use move_model::{ast::TempIndex, model::FunctionEnv};

use crate::{
    function_data_builder::{FunctionDataBuilder, FunctionDataBuilderOptions},
    function_target::{FunctionData, FunctionTarget},
    function_target_pipeline::{FunctionTargetProcessor, FunctionTargetsHolder},
    graph::{Graph, NaturalLoop},
    options::ProverOptions,
    stackless_bytecode::{Bytecode, HavocKind, Label, Operation},
    stackless_control_flow_graph::{BlockContent, BlockId, StacklessControlFlowGraph},
};

/// A fat-loop captures the information of one or more natural loops that share the same loop
/// header. This shared header is called the header of the fat-loop.
///
/// Conceptually, every back edge defines a unique natural loop and different back edges may points
/// to the same loop header (e.g., when there are two "continue" statements in the loop body).
///
/// However, since these natural loops share the same loop header, they share the same loop
/// invariants too and the fat-loop targets (i.e., variables that may be changed in any sub-loop)
/// is the union of loop targets per each natural loop that share the header.
#[derive(Debug, Clone)]
pub struct FatLoop {
    pub val_targets: BTreeSet<TempIndex>,
    pub mut_targets: BTreeMap<TempIndex, bool>,
    pub back_edges: BTreeSet<CodeOffset>,
}

#[derive(Debug, Clone)]
pub struct LoopAnnotation {
    pub fat_loops: BTreeMap<Label, FatLoop>,
}

impl LoopAnnotation {
    fn back_edges_locations(&self) -> BTreeSet<CodeOffset> {
        self.fat_loops
            .values()
            .flat_map(|l| l.back_edges.iter())
            .copied()
            .collect()
    }
}

pub struct LoopAnalysisProcessor {}

impl LoopAnalysisProcessor {
    pub fn new() -> Box<Self> {
        Box::new(LoopAnalysisProcessor {})
    }
}

impl FunctionTargetProcessor for LoopAnalysisProcessor {
    fn process(
        &self,
        _targets: &mut FunctionTargetsHolder,
        func_env: &FunctionEnv,
        data: FunctionData,
        _scc_opt: Option<&[FunctionEnv]>,
    ) -> FunctionData {
        if func_env.is_native() {
            return data;
        }
        let loop_annotation = Self::build_loop_annotation(func_env, &data);
        Self::transform(func_env, data, &loop_annotation)
    }

    fn name(&self) -> String {
        "loop_analysis".to_string()
    }
}

impl LoopAnalysisProcessor {
    /// Perform a loop transformation that eliminate back-edges in a loop and flatten the function
    /// CFG into a directed acyclic graph (DAG).
    ///
    /// The general procedure works as following (assuming the loop invariant expression is L):
    ///
    /// - At the beginning of the loop header (identified by the label bytecode), insert the
    ///   following statements:
    ///     - assert L;
    ///     - havoc T;
    ///     - assume L;
    /// - Create a new dummy block (say, block X) with only the following statements
    ///     - assert L;
    ///     - stop;
    /// - For each backedge in this loop:
    ///     - In the source block of the back edge, replace the last statement (must be a jump or
    ///       branch) with the new label of X.
    fn transform(
        func_env: &FunctionEnv<'_>,
        data: FunctionData,
        loop_annotation: &LoopAnnotation,
    ) -> FunctionData {
        let options = ProverOptions::get(func_env.module_env.env);

        let back_edge_locs = loop_annotation.back_edges_locations();
        let mut builder = FunctionDataBuilder::new_with_options(
            func_env,
            data,
            FunctionDataBuilderOptions {
                no_fallthrough_jump_removal: true,
            },
        );
        let mut goto_fixes = vec![];
        let code = std::mem::take(&mut builder.data.code);
        for (offset, bytecode) in code.into_iter().enumerate() {
            match bytecode {
                Bytecode::Label(attr_id, label) => {
                    builder.emit(bytecode);
                    builder.set_loc_from_attr(attr_id);
                    if let Some(loop_info) = loop_annotation.fat_loops.get(&label) {
                        // havoc all loop targets
                        for (idx, havoc_all) in &loop_info.mut_targets {
                            let havoc_kind = if *havoc_all {
                                HavocKind::MutationAll
                            } else {
                                HavocKind::MutationValue
                            };
                            builder.emit_with(|attr_id| {
                                Bytecode::Call(
                                    attr_id,
                                    vec![*idx],
                                    Operation::Havoc(havoc_kind),
                                    vec![],
                                    None,
                                )
                            });
                        }

                        // trace implicitly reassigned variables after havocking
                        let affected_variables: BTreeSet<_> = loop_info
                            .val_targets
                            .iter()
                            .chain(loop_info.mut_targets.keys())
                            .collect();

                        // Only emit this for user declared locals, not for ones introduced
                        // by stack elimination.
                        let affected_non_temporary_variables: BTreeSet<_> = affected_variables
                            .into_iter()
                            .filter(|&idx| !func_env.is_temporary(*idx))
                            .collect();

                        if affected_non_temporary_variables.is_empty() {
                            // no user declared local is havocked
                            builder.set_next_debug_comment("info: enter loop".to_owned());
                        } else {
                            // show the havocked locals to user
                            let affected_non_temporary_variable_names: Vec<_> =
                                affected_non_temporary_variables
                                    .iter()
                                    .map(|&idx| {
                                        func_env
                                            .symbol_pool()
                                            .string(func_env.get_local_name(*idx))
                                            .to_string()
                                    })
                                    .collect();
                            let joined_variables_names_str =
                                affected_non_temporary_variable_names.join(", ");
                            builder.set_next_debug_comment(format!(
                                "info: enter loop, variable(s) {} havocked and reassigned",
                                joined_variables_names_str
                            ));
                        }

                        // track the new values of havocked user declared locals
                        for idx_ in &affected_non_temporary_variables {
                            let idx = *idx_;
                            builder.emit_with(|id| {
                                Bytecode::Call(
                                    id,
                                    vec![],
                                    Operation::TraceLocal(*idx),
                                    vec![*idx],
                                    None,
                                )
                            });
                        }

                        // after showing the havocked locals and their new values, show the following message
                        if !affected_non_temporary_variables.is_empty() {
                            builder.set_next_debug_comment(
                                "info: loop invariant holds at current state".to_string(),
                            );
                        }
                    }
                }

                _ => {
                    builder.emit(bytecode);
                }
            }
            // mark that the goto labels in this bytecode needs to be updated to a new label
            // representing the invariant-checking block for the loop.
            if back_edge_locs.contains(&(offset as CodeOffset)) {
                goto_fixes.push(builder.data.code.len() - 1);
            }
        }

        // create one invariant-checking block for each fat loop
        let invariant_checker_labels: BTreeMap<_, _> = loop_annotation
            .fat_loops
            .keys()
            .map(|label| (*label, builder.new_label()))
            .collect();

        for label in loop_annotation.fat_loops.keys() {
            let checker_label = invariant_checker_labels.get(label).unwrap();
            builder.set_next_debug_comment(format!(
                "Loop invariant checking block for the loop started with header: L{}",
                label.as_usize()
            ));
            builder.emit_with(|attr_id| Bytecode::Label(attr_id, *checker_label));
            builder.clear_next_debug_comment();

            // stop the checking in proving mode (branch back to loop header for interpretation mode)
            builder.emit_with(|attr_id| {
                if options.for_interpretation {
                    Bytecode::Jump(attr_id, *label)
                } else {
                    Bytecode::Call(attr_id, vec![], Operation::Stop, vec![], None)
                }
            });
        }

        // fix the goto statements in the loop latch blocks
        for code_offset in goto_fixes {
            let updated_goto = match &builder.data.code[code_offset] {
                Bytecode::Jump(attr_id, old_label) => {
                    Bytecode::Jump(*attr_id, *invariant_checker_labels.get(old_label).unwrap())
                }
                Bytecode::Branch(attr_id, if_label, else_label, idx) => {
                    let new_if_label = *invariant_checker_labels.get(if_label).unwrap_or(if_label);
                    let new_else_label = *invariant_checker_labels
                        .get(else_label)
                        .unwrap_or(else_label);
                    Bytecode::Branch(*attr_id, new_if_label, new_else_label, *idx)
                }
                _ => panic!("Expect a branch statement"),
            };
            builder.data.code[code_offset] = updated_goto;
        }

        // we have unrolled the loop into a DAG, and there will be no loop invariants left
        builder.data.loop_invariants.clear();
        builder.data
    }

    /// Collect variables that may be changed during the loop execution.
    ///
    /// The input to this function should include all the sub loops that constitute a fat-loop.
    /// This function will return two sets of variables that represents, respectively,
    /// - the set of values to be havoc-ed, and
    /// - the set of mutations to he havoc-ed and how they should be havoc-ed.
    fn collect_loop_targets(
        cfg: &StacklessControlFlowGraph,
        func_target: &FunctionTarget<'_>,
        sub_loops: &[NaturalLoop<BlockId>],
    ) -> (BTreeSet<TempIndex>, BTreeMap<TempIndex, bool>) {
        let code = func_target.get_bytecode();
        let mut val_targets = BTreeSet::new();
        let mut mut_targets = BTreeMap::new();
        let fat_loop_body: BTreeSet<_> = sub_loops
            .iter()
            .flat_map(|l| l.loop_body.iter())
            .copied()
            .collect();
        for block_id in fat_loop_body {
            for code_offset in cfg
                .instr_indexes(block_id)
                .expect("A loop body should never contain a dummy block")
            {
                let bytecode = &code[code_offset as usize];
                let (bc_val_targets, bc_mut_targets) = bytecode.modifies(func_target);
                val_targets.extend(bc_val_targets);
                for (idx, is_full_havoc) in bc_mut_targets {
                    mut_targets
                        .entry(idx)
                        .and_modify(|v| {
                            *v = *v || is_full_havoc;
                        })
                        .or_insert(is_full_havoc);
                }
            }
        }
        (val_targets, mut_targets)
    }

    /// Collect code offsets that are branch instructions forming loop back-edges
    ///
    /// The input to this function should include all the sub loops that constitute a fat-loop.
    /// This function will return one back-edge location for each sub loop.
    fn collect_loop_back_edges(
        code: &[Bytecode],
        cfg: &StacklessControlFlowGraph,
        header_label: Label,
        sub_loops: &[NaturalLoop<BlockId>],
    ) -> BTreeSet<CodeOffset> {
        sub_loops
            .iter()
            .map(|l| {
                let code_offset = match cfg.content(l.loop_latch) {
                    BlockContent::Dummy => {
                        panic!("A loop body should never contain a dummy block")
                    }
                    BlockContent::Basic { upper, .. } => *upper,
                };
                match &code[code_offset as usize] {
                    Bytecode::Jump(_, goto_label) if *goto_label == header_label => {}
                    Bytecode::Branch(_, if_label, else_label, _)
                        if *if_label == header_label || *else_label == header_label => {}
                    _ => panic!("The latch bytecode of a loop does not branch into the header"),
                };
                code_offset
            })
            .collect()
    }

    /// Find all loops in the function and collect information needed for invariant instrumentation
    /// and loop-to-DAG transformation.
    fn build_loop_annotation(func_env: &FunctionEnv<'_>, data: &FunctionData) -> LoopAnnotation {
        // build for natural loops
        let func_target = FunctionTarget::new(func_env, data);
        let code = func_target.get_bytecode();
        let cfg = StacklessControlFlowGraph::new_forward(code);
        let entry = cfg.entry_block();
        let nodes = cfg.blocks();
        let edges: Vec<(BlockId, BlockId)> = nodes
            .iter()
            .flat_map(|x| {
                cfg.successors(*x)
                    .iter()
                    .map(|y| (*x, *y))
                    .collect::<Vec<(BlockId, BlockId)>>()
            })
            .collect();
        let graph = Graph::new(entry, nodes, edges);
        let natural_loops = graph.compute_reducible().expect(
            "A well-formed Move function is expected to have a reducible control-flow graph",
        );

        // collect shared headers from loops
        let mut fat_headers = BTreeMap::new();
        for single_loop in natural_loops {
            fat_headers
                .entry(single_loop.loop_header)
                .or_insert_with(Vec::new)
                .push(single_loop);
        }

        // build fat loops by label
        let mut fat_loops = BTreeMap::new();
        for (fat_root, sub_loops) in fat_headers {
            // get the label of the scc root
            let label = match cfg.content(fat_root) {
                BlockContent::Dummy => panic!("A loop header should never be a dummy block"),
                BlockContent::Basic { lower, upper: _ } => match code[*lower as usize] {
                    Bytecode::Label(_, label) => label,
                    _ => panic!("A loop header block is expected to start with a Label bytecode"),
                },
            };

            let (val_targets, mut_targets) =
                Self::collect_loop_targets(&cfg, &func_target, &sub_loops);
            let back_edges = Self::collect_loop_back_edges(code, &cfg, label, &sub_loops);

            // done with all information collection.
            fat_loops.insert(
                label,
                FatLoop {
                    val_targets,
                    mut_targets,
                    back_edges,
                },
            );
        }

        LoopAnnotation { fat_loops }
    }
}
