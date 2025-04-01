use std::collections::BTreeSet;

use codespan_reporting::diagnostic::Severity;
use move_model::model::{FunId, FunctionEnv, GlobalEnv, Loc, QualifiedId};

use crate::{
    function_data_builder::FunctionDataBuilder, function_target::{FunctionData, FunctionTarget}, function_target_pipeline::{FunctionTargetProcessor, FunctionTargetsHolder}, stackless_bytecode::{Bytecode, Operation}, stackless_bytecode_generator::StacklessBytecodeGenerator,
};

pub const RESTRICTED_MODULES: [&str; 3] = ["transfer", "event", "emit"];

pub struct SpecPurityAnalysis();

impl SpecPurityAnalysis {
    pub fn new() -> Box<Self> {
        Box::new(Self())
    }

    pub fn find_operation_in_function(&self, target_id: QualifiedId<FunId>, code: &[Bytecode]) -> Option<Operation> {
        for cp in code {
            match cp {
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
        
        None
    }

    // todo: assume dont use graph here, just use bytecode
    pub fn find_modifiable_locations_in_bytecode(&self, code: &[Bytecode], builder: &FunctionDataBuilder, env: &GlobalEnv, skip: &Option<Operation>) -> BTreeSet<Loc> {
        let mut results = BTreeSet::new();
        let mut visited_funcs: BTreeSet<FunId> = BTreeSet::new();

        for cp in code {
            match cp {
                Bytecode::Call(attr, _, operation, _, _) => {
                    if skip.is_some() && skip.clone().unwrap() == *operation {
                        continue;
                    }
                    match operation {
                        Operation::Function(mod_id, func_id, _) => {
                            if !visited_funcs.insert(func_id.clone()) {
                                continue;
                            }

                            let module = env.get_module(*mod_id); 
                            let module_name = env.symbol_pool().string(module.get_name().name());

                            if module_name.as_str() == GlobalEnv::PROVER_MODULE_NAME {
                                continue;
                            }

                            if RESTRICTED_MODULES.contains(&module_name.as_str()) {
                                results.insert(builder.get_loc(*attr)); 
                            }

                            let modif_function = self.find_modifiable_locations_in_bytecode_internal(&module.get_function(*func_id), env, skip, &mut visited_funcs);
                            if modif_function {
                                results.insert(builder.get_loc(*attr)); 
                            }
                        },
                        _ => {}
                    };
                },
                _ => {},
            }
        }
        results
    }

    pub fn find_modifiable_locations_in_bytecode_internal(&self, func_env: &FunctionEnv, env: &GlobalEnv, skip: &Option<Operation>, visited_funcs: &mut BTreeSet<FunId>) -> bool {
        let internal_generator: StacklessBytecodeGenerator<'_> = StacklessBytecodeGenerator::new(&func_env);
        let binding = internal_generator.generate_function();
        let internal_func_target = FunctionTarget::new(&func_env, &binding);
        let internal_code = internal_func_target.get_bytecode();

        for param in func_env.get_parameters() {
            if param.1.is_mutable_reference() {
                return true;
            }
        }

        for cp in internal_code {
            match cp {
                Bytecode::Call(_, _, operation, _, _) => {
                    match operation {
                        Operation::Function(mod_id,func_id, _) => {
                            if !visited_funcs.insert(func_id.clone()) {
                                continue;
                            }


                            let module = env.get_module(*mod_id); 
                            let module_name = env.symbol_pool().string(module.get_name().name());

                            if module_name.as_str() == GlobalEnv::PROVER_MODULE_NAME {
                                continue;
                            }

                            if RESTRICTED_MODULES.contains(&module_name.as_str()) {
                                return true;
                            }

                            let found_issue = self.find_modifiable_locations_in_bytecode_internal(&module.get_function(*func_id), env, skip, visited_funcs);
                            if found_issue {
                                return true;
                            }
                        },
                        _ => {}
                    };
                },
                _ => {},
            }
        }
    
        false
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

        let code = func_target.get_bytecode();
        let builder = FunctionDataBuilder::new(&func_env, data.clone());

        let call_operation = if underlying_func_id.is_some() { self.find_operation_in_function(*underlying_func_id.unwrap(), code) } else { None };
        let modif_locations = self.find_modifiable_locations_in_bytecode(code, &builder, &env, &call_operation);

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
        "spec_purity_analysis".to_string()
    }
}
