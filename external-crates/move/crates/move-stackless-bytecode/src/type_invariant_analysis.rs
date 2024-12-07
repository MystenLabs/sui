use move_model::{exp_generator::ExpGenerator, model::FunctionEnv, ty::Type};

use crate::{
    function_data_builder::FunctionDataBuilder,
    function_target::FunctionData,
    function_target_pipeline::{FunctionTargetProcessor, FunctionTargetsHolder},
    stackless_bytecode::Bytecode,
};

pub struct TypeInvariantAnalysisProcessor();

impl TypeInvariantAnalysisProcessor {
    pub fn new() -> Box<Self> {
        Box::new(Self())
    }
}

impl FunctionTargetProcessor for TypeInvariantAnalysisProcessor {
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

        let mut builder = FunctionDataBuilder::new(func_env, data);
        let code = std::mem::take(&mut builder.data.code);

        builder.set_loc(builder.fun_env.get_loc().at_start());
        for param in 0..builder.fun_env.get_parameter_count() {
            if has_type_inv(targets, &builder.get_local_type(param)) {
                let type_inv_temp = builder.emit_type_inv(param);
                builder.emit_requires(type_inv_temp);
            }
        }

        for bc in code {
            match bc {
                Bytecode::Ret(_, ref rets) => {
                    builder.set_loc(builder.fun_env.get_loc().at_end());
                    for ret in rets {
                        if has_type_inv(targets, &builder.get_local_type(*ret)) {
                            let type_inv_temp = builder.emit_type_inv(*ret);
                            builder.emit_ensures(type_inv_temp);
                        }
                    }
                    for param in 0..builder.fun_env.get_parameter_count() {
                        if builder.get_local_type(param).is_mutable_reference()
                            && has_type_inv(targets, &builder.get_local_type(param))
                        {
                            let type_inv_temp = builder.emit_type_inv(param);
                            builder.emit_ensures(type_inv_temp);
                        }
                    }
                }
                _ => {}
            }
            builder.emit(bc);
        }

        builder.data
    }

    fn name(&self) -> String {
        "type_invariant_analysis".to_string()
    }
}

fn has_type_inv(targets: &FunctionTargetsHolder, ty: &Type) -> bool {
    let ty = ty.skip_reference();
    if let Some((datatype_qid, _)) = ty.get_datatype() {
        if targets.get_inv_by_datatype(&datatype_qid).is_some() {
            return true;
        }
    }
    if ty.is_type_parameter() {
        return true;
    }
    false
}
