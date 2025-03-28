use std::collections::BTreeSet;

use codespan_reporting::diagnostic::Severity;
use move_model::model::{FunId, FunctionEnv, GlobalEnv, Loc, QualifiedId};

use crate::{
    function_data_builder::FunctionDataBuilder, function_target::{FunctionData, FunctionTarget}, function_target_pipeline::{FunctionTargetProcessor, FunctionTargetsHolder}, graph::{DomRelation, Graph}, stackless_bytecode::{Bytecode, Operation}, stackless_control_flow_graph::{BlockContent, BlockId, StacklessControlFlowGraph}
};

pub const RESTRICTED_MODULES: [&str; 3] = ["transfer", "event", "emit"];

pub struct ConditionOrderAnalysisProcessor();

impl ConditionOrderAnalysisProcessor {

    pub fn new() -> Box<Self> {
        Box::new(Self())
    }

    pub fn traverse_successors_and_match_operations(&self, block_id: &BlockId, graph: &Graph<BlockId>, cfg: &StacklessControlFlowGraph, code: &[Bytecode],  builder: &FunctionDataBuilder, targets: &[Operation]) -> BTreeSet<Loc> {
        let mut visited = BTreeSet::new();
        let mut matches = BTreeSet::new();

        visited.insert(cfg.entry_block());
        visited.insert(cfg.exit_block());

        self.traverse_successors_and_match_operations_internal(block_id, block_id, &mut visited, graph, cfg, code, builder, targets, &mut matches);

        matches
    }

    fn traverse_successors_and_match_operations_internal(&self, starting_block_id: &BlockId, block_id: &BlockId, visited: &mut BTreeSet<BlockId>, graph: &Graph<BlockId>, cfg: &StacklessControlFlowGraph, code: &[Bytecode], builder: &FunctionDataBuilder, targets: &[Operation], matches: &mut BTreeSet<Loc>) {
        // Avoid revisiting nodes
        if !visited.insert(*block_id)  {
            return;
        }

        if starting_block_id != block_id {
            let loc = self.fing_node_operation(*block_id, cfg, code, targets, builder);
            if loc.is_some() {
                matches.insert(loc.unwrap());
            }
        }

        for successor in graph.successors[block_id].clone().iter() {
            self.traverse_successors_and_match_operations_internal(starting_block_id, &successor, visited, graph, cfg, code, builder, targets, matches);
        }
    }

    pub fn traverse_predecessors_and_match_operations(&self, block_id: &BlockId, graph: &Graph<BlockId>, cfg: &StacklessControlFlowGraph, code: &[Bytecode], builder: &FunctionDataBuilder, targets: &[Operation]) -> BTreeSet<Loc> {
        let mut visited = BTreeSet::new();
        let mut matches = BTreeSet::new();

        visited.insert(cfg.entry_block());
        visited.insert(cfg.exit_block());

        self.traverse_predecessors_and_match_operations_internal(block_id, block_id, &mut visited, graph, cfg, code, builder, targets, &mut matches);

        matches
    }

    fn traverse_predecessors_and_match_operations_internal(&self, starting_block_id: &BlockId, block_id: &BlockId, visited: &mut BTreeSet<BlockId>, graph: &Graph<BlockId>, cfg: &StacklessControlFlowGraph, code: &[Bytecode], builder: &FunctionDataBuilder, targets: &[Operation], matches: &mut BTreeSet<Loc>) {
        // Avoid revisiting nodes
        if !visited.insert(*block_id) {
            return;
        }

        if starting_block_id != block_id {
            let loc = self.fing_node_operation(*block_id, cfg, code, targets, builder);
            if loc.is_some() {
                matches.insert(loc.unwrap());
            }
        }

        // Recursively visit each successor
        for predecessor in graph.predecessors[block_id].clone().iter() {
            self.traverse_predecessors_and_match_operations_internal(starting_block_id,&predecessor, visited, graph, cfg, code, builder, targets, matches);
        }
    }

    pub fn find_node_by_func_id(&self, target_id: QualifiedId<FunId>, graph: &Graph<BlockId>, code: &[Bytecode], cfg: &StacklessControlFlowGraph) -> (Option<BlockId>, Option<Operation>, bool) {
        let mut call_node_id: Option<BlockId> = None;
        let mut call_operation: Option<Operation> = None;
        let mut multiple = false;
        for node in graph.nodes.clone() {
            match cfg.content(node) {
                BlockContent::Dummy => {},
                BlockContent::Basic { lower, upper } => {
                    for position in *lower..*upper {
                        match &code[position as usize] {
                            Bytecode::Call(_, _, operation, _, _) => {
                                match operation {
                                    Operation::Function(mod_id,fun_id, _) => {
                                        let callee_id = mod_id.qualified(*fun_id);
                                        if callee_id == target_id {
                                            if call_node_id.is_some() {
                                                multiple = true;
                                            }
                                            call_node_id = Some(node);
                                            call_operation = Some(operation.clone());
                                        }
                                    },
                                    _ => {}
                                };
                            },
                            _ => {},
                        }
                    }
                },
            };                
        }

        (call_node_id, call_operation, multiple)
    }

    pub fn fing_node_operation(&self, block_id: BlockId, cfg: &StacklessControlFlowGraph, code: &[Bytecode], targets: &[Operation], builder: &FunctionDataBuilder) -> Option<Loc> {
        match cfg.content(block_id) {
            BlockContent::Dummy => {},
            BlockContent::Basic { lower, upper } => {
                for position in *lower..*upper {
                    match &code[position as usize] {
                        Bytecode::Call(attr, _, opr, _, _) => {
                            if targets.contains(opr) {
                                return Some(builder.get_loc(*attr));
                            }
                        },
                        _ => {},
                    }
                }
            }
        }
    
        return None;
    }

    pub fn fing_operations_before_after_operation_in_node(&self, block_id: &BlockId, operation: &Operation, cfg: &StacklessControlFlowGraph, code: &[Bytecode], builder: &FunctionDataBuilder, before_targets: &[Operation], after_targets: &[Operation]) -> (BTreeSet<Loc>, BTreeSet<Loc>) {
        let mut befores = BTreeSet::new();
        let mut afters = BTreeSet::new();
        let mut matched = false;

        match cfg.content(*block_id) {
            BlockContent::Dummy => {},
            BlockContent::Basic { lower, upper } => {
                for position in *lower..*upper {
                    match &code[position as usize] {
                        Bytecode::Call(attr, _, opr, _, _) => {
                            if opr == operation {
                                matched = true;
                            }

                            if !matched && before_targets.contains(opr) {
                                befores.insert(builder.get_loc(*attr));
                            }

                            if matched && after_targets.contains(opr) {
                                afters.insert(builder.get_loc(*attr));
                            }
                        },
                        _ => {},
                    }
                }
            }
        }
    
        return (befores, afters);
    }
    
    pub fn find_modifiable_locations_in_graph(&self, graph: &Graph<BlockId>, code: &[Bytecode], cfg: &StacklessControlFlowGraph, builder: &FunctionDataBuilder, env: &GlobalEnv, skip: &Operation) -> BTreeSet<Loc> {
        let mut results = BTreeSet::new();
        for node in graph.nodes.clone() {
            match cfg.content(node) {
                BlockContent::Dummy => {},
                BlockContent::Basic { lower, upper } => {
                    for position in *lower..(*upper + 1) {
                        match &code[position as usize] {
                            Bytecode::Call(attr, _, operation, _, _) => {
                                if skip != operation {
                                    match operation {
                                        Operation::Function(mod_id,fun_id, types) => {
                                            let module = env.get_module(*mod_id); 
                                            let module_name = env.symbol_pool().string(module.get_name().name());

                                            if RESTRICTED_MODULES.contains(&module_name.as_str()) {
                                                results.insert(builder.get_loc(*attr)); 
                                            }
                                            
                                            for param_type in types {
                                                if param_type.is_mutable_reference() {
                                                    results.insert(builder.get_loc(*attr)); 
                                                }
                                            }

                                            println!("{:?} {} names", env.symbol_pool().string(module.get_name().name()), module.get_function(*fun_id).get_name_str());
                                        },
                                        _ => {}
                                    };
                                }
                            },
                            _ => {},
                        }
                    }
                },
            };                
        }

        results
    }
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

        let env = func_env.module_env.env;
        let func_target = FunctionTarget::new(func_env, &data);

        let underlying_func_id = targets
            .function_specs()
            .iter()
            .find_map(|(id, func)| (func_env.get_qualified_id() == *id).then_some(*func));

        if underlying_func_id.is_none() {
            env.diag(
                Severity::Error,
                &func_env.get_loc(),
                "Spec underlying func is not found",
            );

            return  data;
        }

        let underlying_func = func_env.module_env.get_function(underlying_func_id.unwrap().id);

        let spec_params = func_env.get_parameters();
        let underlying_params = underlying_func.get_parameters();

        if spec_params.len() != underlying_params.len() {
            env.diag(
                Severity::Error,
                &func_env.get_loc(),
                "Spec function have differ params count than underlying func",
            );

            return data;
        }

        for i in 0..spec_params.len() {
            if spec_params[i].0 != underlying_params[i].0 {
                env.diag(
                    Severity::Warning,
                    &func_env.get_loc(),
                    "Spec function have differ params names than underlying func",
                );
            }

            if spec_params[i].1 != underlying_params[i].1 {
                env.diag(
                    Severity::Error,
                    &func_env.get_loc(),
                    "Spec function have differ params type than underlying func",
                );

                return data;
            }
        }

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

        let (call_node_id, call_operation, multiple_calls) = self.find_node_by_func_id(underlying_func.get_qualified_id(), &graph, code, &cfg);

        if !call_node_id.is_some() {
            env.diag(
                Severity::Error,
                &func_env.get_loc(),
                "Consider add function call to spec",
            );

            return data;
        }

        if multiple_calls {
            env.diag(
                Severity::Error,
                &func_env.get_loc(),
                "Underlying func is calling few times",
            );

            return data;
        }

        let modif_locations = self.find_modifiable_locations_in_graph(&graph, code, &cfg, &builder, &env, &call_operation.clone().unwrap());

        for loc in modif_locations.iter() {
            env.diag(
                Severity::Error,
                loc,
                "Consider removing non-pure calls form spec",
            );
        }

        if modif_locations.iter().len() > 0 {
            return data;
        }

        let dom_relations = DomRelation::new(&graph);
        let is_dominated = dom_relations.is_dominated_by(cfg.exit_block(), call_node_id.unwrap());

        if !is_dominated {
            env.diag(
                Severity::Error,
                &func_env.get_loc(),
                "Underlying func is not calling in all execution ways",
            );

            return data;
        }

        let postconditions = [Operation::apply_fun_qid(&func_env.module_env.env.ensures_qid(), vec![])];

        let preconditions = [
            Operation::apply_fun_qid(&func_env.module_env.env.requires_qid(), vec![]),
            Operation::apply_fun_qid(&func_env.module_env.env.asserts_qid(), vec![]),
        ];

        let mut pre_matches_traversed = self.traverse_successors_and_match_operations(&call_node_id.unwrap(), &graph, &cfg, code, &builder, &preconditions);
        let mut post_matches_traversed = self.traverse_predecessors_and_match_operations(&call_node_id.unwrap(), &graph, &cfg, code, &builder, &postconditions);
        let (mut pre_matches, mut post_matches) = self.fing_operations_before_after_operation_in_node(&call_node_id.unwrap(), &call_operation.unwrap(), &cfg, code, &builder, &postconditions, &preconditions);

        pre_matches.append(&mut pre_matches_traversed);
        post_matches.append(&mut post_matches_traversed);

        for loc in pre_matches.iter() {
            env.diag(
                Severity::Warning,
                loc,
                "Consider moving pre-condition before function call",
            );
        }

        for loc in post_matches.iter() {
            env.diag(
                Severity::Warning,
                loc,
                "Consider moving post-condition after target function call",
            );
        }
        data
    }

    fn name(&self) -> String {
        "conditions_order_analysis".to_string()
    }
}
