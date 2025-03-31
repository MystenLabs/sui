use std::collections::BTreeSet;

use codespan_reporting::diagnostic::Severity;
use move_model::model::{FunId, FunctionEnv, GlobalEnv, Loc, QualifiedId};

use crate::{
    function_data_builder::FunctionDataBuilder, function_target::{FunctionData, FunctionTarget}, function_target_pipeline::{FunctionTargetProcessor, FunctionTargetsHolder}, graph::Graph, stackless_bytecode::{Bytecode, Operation}, stackless_control_flow_graph::{BlockContent, BlockId, StacklessControlFlowGraph}
};

pub const RESTRICTED_MODULES: [&str; 3] = ["transfer", "event", "emit"];

pub struct SpecPurityAnalysis();

impl SpecPurityAnalysis {
    pub fn new() -> Box<Self> {
        Box::new(Self())
    }

    pub fn find_operation_by_func_id(&self, target_id: QualifiedId<FunId>, graph: &Graph<BlockId>, code: &[Bytecode], cfg: &StacklessControlFlowGraph) -> Option<Operation> {
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
                                            return Some(operation.clone());
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

        None
    }

    // todo: assume dont use graph here, just use bytecode
    pub fn find_modifiable_locations_in_graph(&self, graph: &Graph<BlockId>, code: &[Bytecode], cfg: &StacklessControlFlowGraph, builder: &FunctionDataBuilder, env: &GlobalEnv, skip: &Option<Operation>) -> BTreeSet<Loc> {
        let mut results = BTreeSet::new();
        for node in graph.nodes.clone() {
            match cfg.content(node) {
                BlockContent::Dummy => {},
                BlockContent::Basic { lower, upper } => {
                    for position in *lower..(*upper + 1) {
                        match &code[position as usize] {
                            Bytecode::Call(attr, _, operation, _, _) => {
                                if skip.is_some() && skip.clone().unwrap() == *operation {
                                    continue;
                                }
                                match operation {
                                    Operation::Function(mod_id,_, types) => {
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

        results
    }
}

impl FunctionTargetProcessor for SpecPurityAnalysis {
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

        let call_operation = self.find_operation_by_func_id(underlying_func.get_qualified_id(), &graph, code, &cfg);

        let modif_locations = self.find_modifiable_locations_in_graph(&graph, code, &cfg, &builder, &env, &call_operation);

        for loc in modif_locations.iter() {
            env.diag(
                Severity::Error,
                loc,
                "Consider removing non-pure calls form spec",
            );
        }

        data
    }

    fn name(&self) -> String {
        "conditions_order_analysis".to_string()
    }
}
