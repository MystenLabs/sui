use std::collections::BTreeSet;

use codespan_reporting::diagnostic::Severity;
use move_model::model::{FunId, FunctionEnv, GlobalEnv, Loc, QualifiedId};

use crate::{function_target::{FunctionData, FunctionTarget}, function_target_pipeline::{FunctionTargetProcessor, FunctionTargetsHolder, FunctionVariant}, stackless_bytecode::{Bytecode, Operation}};

pub const RESTRICTED_MODULES: [&str; 3] = ["transfer", "event", "emit"];

#[derive(Clone)]
pub struct PurityVerificationInfo {
    pub is_pure: bool,
}

impl Default for PurityVerificationInfo {
    fn default() -> Self {
        Self { is_pure: true }
    }
}

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

    pub fn find_modifiable_locations_in_function(
        &self, 
        code: &[Bytecode], 
        func_env: &FunctionEnv,
        targets: &FunctionTargetsHolder,
        target: &FunctionTarget, 
        env: &GlobalEnv, 
        skip: &Option<Operation>,
    ) -> BTreeSet<Loc> {
        let mut results = BTreeSet::new();

        if !targets.is_spec(&func_env.get_qualified_id()) {
            for param in func_env.get_parameters() {
                if param.1.is_mutable_reference() {
                    results.insert(func_env.get_loc()); 
                }
            }
        }

        for cp in code {
            match cp {
                Bytecode::Call(attr, _, operation, _, _) => {
                    if skip.is_some() && skip.clone().unwrap() == *operation {
                        continue;
                    }
                    match operation {
                        Operation::Function(mod_id, func_id, _) => {
                            let module = env.get_module(*mod_id); 
                            let module_name = env.symbol_pool().string(module.get_name().name());

                            if module_name.as_str() == GlobalEnv::PROVER_MODULE_NAME || module_name.as_str() == GlobalEnv::SPEC_MODULE_NAME {
                                continue;
                            }

                            if RESTRICTED_MODULES.contains(&module_name.as_str()) {
                                results.insert(target.get_bytecode_loc(*attr)); 
                            }

                            let internal_data = targets.get_data(&mod_id.qualified(*func_id), &FunctionVariant::Baseline);
                            if internal_data.is_none() {
                                continue;
                            }
                                
                            let annotation = internal_data
                                .unwrap()
                                .annotations.get::<PurityVerificationInfo>()
                                .expect("Function expect to be already scanned");

                            if !annotation.is_pure {
                                results.insert(target.get_bytecode_loc(*attr)); 
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

    fn analyse(&self, func_env: &FunctionEnv, targets: &FunctionTargetsHolder, data: &FunctionData) -> PurityVerificationInfo {
        let is_spec = targets.is_function_spec(&func_env.get_qualified_id());
        let env = func_env.module_env.env;
        let func_target = FunctionTarget::new(func_env, data);

        let underlying_func_id = targets.get_fun_by_spec(&func_env.get_qualified_id());

        let code = func_target.get_bytecode();

        let call_operation = if underlying_func_id.is_some() { self.find_operation_in_function(*underlying_func_id.unwrap(), code) } else { None };
        let modif_locations = self.find_modifiable_locations_in_function(code, &func_env, targets, &func_target, &env, &call_operation);

        if is_spec {
            for loc in modif_locations.iter() {
                env.diag(
                    Severity::Error,
                    loc,
                    "Consider removing non-pure calls form spec",
                );
            }
        }

        PurityVerificationInfo { is_pure: modif_locations.len() == 0 }
    }
}

impl FunctionTargetProcessor for SpecPurityAnalysis {
    fn process(
        &self,
        targets: &mut FunctionTargetsHolder,
        func_env: &FunctionEnv,
        mut data: FunctionData,
        scc_opt: Option<&[FunctionEnv]>,
    ) -> FunctionData {
        let annotation = data
            .annotations
            .get::<PurityVerificationInfo>();

        let annotation_data = if annotation.is_some() { 
            annotation.unwrap().clone()
         } else { 
            self.analyse(func_env, targets, &data)
        };
    
        let fixedpoint = match scc_opt {
            None => true,
            Some(_) => match data.annotations.get::<PurityVerificationInfo>() {
                None => false,
                Some(old_annotation) => old_annotation.is_pure == annotation_data.is_pure
            },
        };

        data.annotations.set::<PurityVerificationInfo>(annotation_data, fixedpoint);

        data
    }

    fn name(&self) -> String {
        "spec_purity_analysis".to_string()
    }
}
