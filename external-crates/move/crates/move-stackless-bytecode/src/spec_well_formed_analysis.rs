use std::{collections::BTreeSet, vec};

use codespan_reporting::diagnostic::Severity;
use move_model::{model::{FunId, FunctionEnv, Loc, QualifiedId}, symbol::Symbol, ty::Type};

use crate::{
    function_data_builder::FunctionDataBuilder, function_target::{FunctionData, FunctionTarget}, function_target_pipeline::{FunctionTargetProcessor, FunctionTargetsHolder}, graph::{DomRelation, Graph}, stackless_bytecode::{Bytecode, Operation}, stackless_control_flow_graph::{BlockContent, BlockId, StacklessControlFlowGraph}
};

pub struct SpecWellFormedAnalysisProcessor();

impl SpecWellFormedAnalysisProcessor {

    pub fn new() -> Box<Self> {
        Box::new(Self())
    }

    pub fn traverse_and_match_operations(&self, is_forward: bool, block_id: &BlockId, graph: &Graph<BlockId>, cfg: &StacklessControlFlowGraph, code: &[Bytecode],  builder: &FunctionDataBuilder, targets: &[Operation]) -> BTreeSet<Loc> {
        let mut visited = BTreeSet::new();
        let mut matches = BTreeSet::new();

        visited.insert(cfg.entry_block());
        visited.insert(cfg.exit_block());

        self.traverse_and_match_operations_internal(is_forward, block_id, block_id, &mut visited, graph, cfg, code, builder, targets, &mut matches);

        matches
    }

    fn traverse_and_match_operations_internal(&self, is_forward: bool, starting_block_id: &BlockId, block_id: &BlockId, visited: &mut BTreeSet<BlockId>, graph: &Graph<BlockId>, cfg: &StacklessControlFlowGraph, code: &[Bytecode], builder: &FunctionDataBuilder, targets: &[Operation], matches: &mut BTreeSet<Loc>) {
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

        let nodes = if is_forward { graph.successors[block_id].clone() } else { graph.predecessors[block_id].clone() };

        for successor in nodes.iter() {
            self.traverse_and_match_operations_internal(is_forward, starting_block_id, &successor, visited, graph, cfg, code, builder, targets, matches);
        }
    }

    pub fn find_node_by_func_id(&self, target_id: QualifiedId<FunId>, graph: &Graph<BlockId>, code: &[Bytecode], cfg: &StacklessControlFlowGraph) -> (Option<(BlockId, Operation, Vec<usize>, Vec<usize>, Vec<Type>)>, bool) {
        let mut multiple = false;
        let mut result = None;

        for node in graph.nodes.clone() {
            match cfg.content(node) {
                BlockContent::Dummy => {},
                BlockContent::Basic { lower, upper } => {
                    for position in *lower..*upper {
                        match &code[position as usize] {
                            Bytecode::Call(_, dsts, operation, srcs, _) => {
                                match operation {
                                    Operation::Function(mod_id,fun_id, type_params) => {
                                        let callee_id = mod_id.qualified(*fun_id);
                                        if callee_id == target_id {
                                            if result.is_some() {
                                                multiple = true;
                                            }

                                            result = Some((node, operation.clone(), dsts.clone(), srcs.clone(), type_params.clone()));
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

        (result, multiple)
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

    pub fn get_return_variables(&self, func_env: &FunctionEnv, code: &[Bytecode]) -> Vec<Vec<Symbol>> {
        // using matrix to cover all possible returns with params
        let mut results = vec!();
        for cp in code.iter() {
            match cp {
                Bytecode::Ret(_, srcs) => {
                    let mut result: Vec<Symbol> = vec!();
                    for idx in srcs.clone() {
                        let lc = func_env.get_local_name(idx);
                        result.push(lc);
                    }
                    
                    results.push(result);
                } 
                _ => {}
            }
        }

        results
    }
}

impl FunctionTargetProcessor for SpecWellFormedAnalysisProcessor {
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

        let underlying_func_id = targets.get_fun_by_spec(&func_env.get_qualified_id());

        if underlying_func_id.is_none() {
            return  data;
        }

        let underlying_func = env.get_function(underlying_func_id.unwrap().clone());

        // Signatures Checking

        let spec_params = func_env.get_parameters();
        let underlying_params = underlying_func.get_parameters();

        let spec_type_params = func_env.get_type_parameters();
        let underlying_type_params = underlying_func.get_type_parameters();

        if spec_type_params.len() != underlying_type_params.len() {
            env.diag(
                Severity::Error,
                &func_env.get_loc(),
                "Spec function have differ type params count than underlying func",
            );

            return data;
        }

        if spec_params.len() != underlying_params.len() {
            env.diag(
                Severity::Error,
                &func_env.get_loc(),
                "Spec function have differ params count than underlying func",
            );

            return data;
        }

        for i in 0..spec_params.len() {
            if spec_params[i].1 != underlying_params[i].1 {
                env.diag(
                    Severity::Error,
                    &func_env.get_loc(),
                    "Spec function have differ params type than underlying func",
                );

                return data;
            }

            if spec_params[i].0 != underlying_params[i].0 {
                let underlying_param_name = env.symbol_pool().string( underlying_params[i].0);
                if !underlying_param_name.starts_with('_') {
                    env.diag(
                        Severity::Warning,
                        &func_env.get_loc(),
                        "Spec function signature have differ params name than underlying func",
                    );
                }
            }
        }

        for i in 0..spec_type_params.len() {
            if spec_type_params[i].1 != underlying_type_params[i].1 {
                env.diag(
                    Severity::Error,
                    &func_env.get_loc(),
                    "Spec function have differ type params abilities than underlying func",
                );

                return data;
            }

            if spec_type_params[i].0 != underlying_type_params[i].0 {
                env.diag(
                    Severity::Warning,
                    &func_env.get_loc(),
                    "Spec function signature have differ type params name than underlying func",
                );
            }
        }

        let spec_return_types = func_env.get_return_types();
        let underlying_return_types = underlying_func.get_return_types();

        if spec_return_types.len() != underlying_return_types.len() {
            env.diag(
                Severity::Error,
                &func_env.get_loc(),
                "Spec function have differ return types count than underlying func",
            );

            return data;
        }

        for i in 0..spec_return_types.len() {
            if spec_return_types[i] != underlying_return_types[i] {
                env.diag(
                    Severity::Error,
                    &func_env.get_loc(),
                    "Spec function have differ return types than underlying func",
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

        let (call_data, multiple_calls) = self.find_node_by_func_id(underlying_func.get_qualified_id(), &graph, code, &cfg);

        if !call_data.is_some() {
            if !underlying_func.is_native() {
                env.diag(
                    Severity::Error,
                    &func_env.get_loc(),
                    "Consider add function call to spec",
                );
            }

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

        let (call_node_id, call_operation, outputs, inputs, type_param_args) = call_data.unwrap();

        // Arguments Checking

        for idx in 0..type_param_args.len() {
            match type_param_args[idx] {
                Type::TypeParameter(id) => {
                    if idx as u16 != id {
                        env.diag(
                            Severity::Error,
                            &func_env.get_loc(),
                            "Underlying func accepting type param from spec in wrong order",
                        );

                        return data;
                    }
                },
                _ => {
                    env.diag(
                        Severity::Error,
                        &func_env.get_loc(),
                        "Underlying func not accepting type param from spec",
                    );

                    return data;
                },
            }
        }

        let spec_params_symbols: Vec<Symbol> = spec_params.iter().map(|sd| sd.0).collect(); 
        for src in inputs {
            let lc = func_target.get_local_name(src);

            if !spec_params_symbols.contains(&lc) { // => if input variable for underlying function is not spec parameter
                env.diag(
                    Severity::Error,
                    &func_env.get_loc(),
                    "Underlying func input var is not a function parameter",
                );

                return data;
            }
        }

        let return_symbols_matrix = self.get_return_variables(func_env, code);
        let output_symbols: Vec<Symbol> = outputs.iter()
            .map(|idx| func_target.get_local_name(*idx))
            .collect(); 

        for return_symbols in return_symbols_matrix {
            for rs in return_symbols {
                if !output_symbols.contains(&rs) { // => if return variable of spec is not result of underlying func call
                    env.diag(
                        Severity::Error,
                        &func_env.get_loc(),
                        "Underlying func result var is not returned from spec",
                    );

                    return data;
                }
            }
        }

        let dom_relations = DomRelation::new(&graph);
        let is_dominated = dom_relations.is_dominated_by(cfg.exit_block(), call_node_id);

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

        let mut pre_matches_traversed = self.traverse_and_match_operations(true, &call_node_id, &graph, &cfg, code, &builder, &preconditions);
        let mut post_matches_traversed = self.traverse_and_match_operations(false, &call_node_id, &graph, &cfg, code, &builder, &postconditions);
        let (mut pre_matches, mut post_matches) = self.fing_operations_before_after_operation_in_node(&call_node_id, &call_operation, &cfg, code, &builder, &postconditions, &preconditions);

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
        "spec_well_formed_analysis".to_string()
    }
}
