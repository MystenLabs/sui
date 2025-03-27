use std::collections::BTreeSet;

use codespan_reporting::diagnostic::Severity;
use move_model::model::{FunId, FunctionEnv, Loc, QualifiedId};

use crate::{
    function_data_builder::FunctionDataBuilder, function_target::{FunctionData, FunctionTarget}, function_target_pipeline::{FunctionTargetProcessor, FunctionTargetsHolder}, graph::Graph, stackless_bytecode::{Bytecode, Operation}, stackless_control_flow_graph::{BlockContent, BlockId, StacklessControlFlowGraph}
};

pub struct ConditionOrderAnalysisProcessor();

impl ConditionOrderAnalysisProcessor {
    pub fn new() -> Box<Self> {
        Box::new(Self())
    }
}

fn fing_node_operation(block_id: BlockId, cfg: &StacklessControlFlowGraph, code: &[Bytecode], targets: &[Operation], builder: &FunctionDataBuilder) -> Option<Loc> {
    match cfg.content(block_id) {
        BlockContent::Dummy => {},
        BlockContent::Basic { lower, upper: _ } => {
            match &code[*lower as usize] {
                Bytecode::Call(attr, _, opr, _, _) => {
                    if targets.contains(opr) {
                        return Some(builder.get_loc(*attr));
                    }
                },
                _ => {},
            }
        }
    }

    return None;
}

fn traverse_successors_and_match_operations(block_id: &BlockId, graph: &Graph<BlockId>, cfg: &StacklessControlFlowGraph, code: &[Bytecode],  builder: &FunctionDataBuilder, targets: &[Operation]) -> BTreeSet<Loc> {
    let mut visited = BTreeSet::new();
    let mut matches = BTreeSet::new();
 
    traverse_successors_and_match_operations_internal(block_id, &mut visited, graph, cfg, code, builder, targets, &mut matches);

    matches
}

fn traverse_successors_and_match_operations_internal(block_id: &BlockId, visited: &mut BTreeSet<BlockId>, graph: &Graph<BlockId>, cfg: &StacklessControlFlowGraph, code: &[Bytecode], builder: &FunctionDataBuilder, targets: &[Operation], matches: &mut BTreeSet<Loc>) {
    // Avoid revisiting nodes
    if !visited.insert(*block_id) {
        return;
    }

    let loc = fing_node_operation(*block_id, cfg, code, targets, builder);
    if loc.is_some() {
        matches.insert(loc.unwrap());
    }

    for successor in graph.successors[block_id].clone().iter() {
        traverse_successors_and_match_operations_internal(&successor, visited, graph, cfg, code, builder, targets, matches);
    }
}

fn traverse_predecessors_and_match_operations(block_id: &BlockId, graph: &Graph<BlockId>, cfg: &StacklessControlFlowGraph, code: &[Bytecode], builder: &FunctionDataBuilder, targets: &[Operation]) -> BTreeSet<Loc> {
    let mut visited = BTreeSet::new();
    let mut matches = BTreeSet::new();
 
    traverse_predecessors_and_match_operations_internal(block_id, &mut visited, graph, cfg, code, builder, targets, &mut matches);

    matches
}

fn traverse_predecessors_and_match_operations_internal(block_id: &BlockId, visited: &mut BTreeSet<BlockId>, graph: &Graph<BlockId>, cfg: &StacklessControlFlowGraph, code: &[Bytecode], builder: &FunctionDataBuilder, targets: &[Operation], matches: &mut BTreeSet<Loc>) {
    // Avoid revisiting nodes
    if !visited.insert(*block_id) {
        return;
    }

    let loc = fing_node_operation(*block_id, cfg, code, targets, builder);
    if loc.is_some() {
        matches.insert(loc.unwrap());
    }

    // Recursively visit each successor
    for predecessor in graph.predecessors[block_id].clone().iter() {
        traverse_predecessors_and_match_operations_internal(&predecessor, visited, graph, cfg, code, builder, targets, matches);
    }
}

fn find_node_by_func_id(target_id: QualifiedId<FunId>, graph: &Graph<BlockId>, code: &[Bytecode], cfg: &StacklessControlFlowGraph) -> Option<BlockId> {
    let mut call_node_id: Option<BlockId> = None;
    for node in graph.nodes.clone() {
        match cfg.content(node) {
            BlockContent::Dummy => {},
            BlockContent::Basic { lower, upper: _ } => {
                match &code[*lower as usize] {
                    Bytecode::Call(_, _, operation, _, _) => {
                        match operation {
                            Operation::Function(mod_id,fun_id, _) => {
                                let callee_id = mod_id.qualified(*fun_id);
                                if callee_id == target_id {
                                    call_node_id = Some(node);
                                }
                            },
                            _ => {}
                        };
                    },
                    _ => {},
                }
            },
        };                
    }

    return call_node_id;
}

impl FunctionTargetProcessor for ConditionOrderAnalysisProcessor {
    fn process(
        &self,
        targets: &mut FunctionTargetsHolder,
        func_env: &FunctionEnv,
        data: FunctionData,
        _scc_opt: Option<&[FunctionEnv]>,
    ) -> FunctionData {
        if !targets.is_spec(&func_env.get_qualified_id()) {
            // only need to do this for spec functions
            return data;
        }

        let postconditions = [Operation::apply_fun_qid(&func_env.module_env.env.ensures_qid(), vec![])];

        let preconditions = [
            Operation::apply_fun_qid(&func_env.module_env.env.requires_qid(), vec![]),
            Operation::apply_fun_qid(&func_env.module_env.env.asserts_qid(), vec![]),
        ];

        let env = func_env.module_env.env;
        let func_target = FunctionTarget::new(func_env, &data);
        let code = func_target.get_bytecode();
        let cfg: StacklessControlFlowGraph = StacklessControlFlowGraph::new_forward(code);
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
        let graph: Graph<u16> = Graph::new(entry, nodes, edges);

        let builder = FunctionDataBuilder::new(&func_env, data.clone());

        let underlying_func = targets
            .function_specs()
            .iter()
            .find_map(|(id, func)| (func_env.get_qualified_id() == *id).then_some(*func))
            .expect("Every spec should have a function");

        let call_node_id: Option<BlockId> = find_node_by_func_id(underlying_func, &graph, code, &cfg);

        if !call_node_id.is_some() {
            env.diag(
                Severity::Error,
                &func_env.get_loc(),
                "Consider add function call to spec",
            );

            return data;
        }

        let matches_s = traverse_successors_and_match_operations(&call_node_id.unwrap(), &graph, &cfg, code, &builder, &preconditions);
        let matches_p = traverse_predecessors_and_match_operations(&call_node_id.unwrap(), &graph, &cfg, code, &builder, &postconditions);

        for loc in matches_s.iter() {
            env.diag(
                Severity::Warning,
                loc,
                "Consider moving pre-condition before function call",
            );
        }

        for loc in matches_p.iter() {
            env.diag(
                Severity::Warning,
                loc,
                "Consider moving post-condition before target function call",
            );
        }
        data
    }

    fn name(&self) -> String {
        "conditions_order_analysis".to_string()
    }
}
