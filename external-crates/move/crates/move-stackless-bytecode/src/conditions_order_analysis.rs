use codespan_reporting::diagnostic::Severity;
use itertools::Itertools;
use move_model::model::FunctionEnv;

use crate::{
    function_target::FunctionData,
    function_target_pipeline::{FunctionTargetProcessor, FunctionTargetsHolder},
};

pub struct ConditionOrderAnalysisProcessor();

impl ConditionOrderAnalysisProcessor {
    pub fn new() -> Box<Self> {
        Box::new(Self())
    }
}

const PRE_CONDITIONS: [&str; 2] = ["asserts", "requires"];
const POST_CONDITIONS: [&str; 1] = ["ensures"];
const PROVER_MODULE: &str = "prover";

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

        for target in targets.function_specs() {
            let mut func_call_reached = false;
            let mut is_post_condition_reached = false;

            let spec_func= env.get_function(*target.0);
            let func_call_exists = spec_func.get_called_functions().iter().contains(target.1);

            for called in spec_func.get_called_functions_iter() {
                let module_name = called.module_env.symbol_pool().string(called.module_env.get_name().name());
                let func_name = called.get_name_str();
                
                if module_name.as_str() == PROVER_MODULE {
                    if PRE_CONDITIONS.contains(&func_name.as_str()) {
                        if func_call_reached {
                            env.diag(
                                Severity::Warning,
                                &spec_func.get_loc(),
                                "Consider moving pre-condition before function call",
                            );
                        }

                        if is_post_condition_reached {
                            env.diag(
                                Severity::Warning,
                                &spec_func.get_loc(),
                                "Consider moving pre-condition before post-condition",
                            );
                        }
                    }
                    
                    if POST_CONDITIONS.contains(&func_name.as_str()) {
                        is_post_condition_reached = true;

                        if !func_call_reached && func_call_exists {
                            env.diag(
                                Severity::Warning,
                                &spec_func.get_loc(),
                                "Consider moving post-condition after func call",
                            );
                        }
                    }
                }
                if called.get_qualified_id() == *target.1 {
                    func_call_reached = true;
                }
            }
        }

        data
    }

    fn name(&self) -> String {
        "conditions_order_analysis".to_string()
    }
}
