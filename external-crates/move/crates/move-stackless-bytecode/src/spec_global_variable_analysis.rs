use itertools::Itertools;
use std::collections::BTreeSet;

use move_model::{
    model::{FunId, FunctionEnv, GlobalEnv, QualifiedId},
    ty::Type,
};

use crate::{
    function_target::{FunctionData, FunctionTarget},
    function_target_pipeline::{FunctionTargetProcessor, FunctionTargetsHolder, FunctionVariant},
    stackless_bytecode::{Bytecode, Operation},
};

/// The environment extension computed by this analysis.
#[derive(Clone, Debug)]
pub struct SpecGlobalVariableInfo {
    imm_vars: BTreeSet<Vec<Type>>,
    mut_vars: BTreeSet<Vec<Type>>,
}

impl SpecGlobalVariableInfo {
    pub fn imm_vars(&self) -> &BTreeSet<Vec<Type>> {
        &self.imm_vars
    }

    pub fn mut_vars(&self) -> &BTreeSet<Vec<Type>> {
        &self.mut_vars
    }

    pub fn all_vars(&self) -> impl Iterator<Item = Vec<Type>> + '_ {
        self.imm_vars.union(&self.mut_vars).cloned()
    }

    pub fn union(&self, other: &Self) -> Self {
        Self {
            imm_vars: self.imm_vars.union(&other.imm_vars).cloned().collect(),
            mut_vars: self.mut_vars.union(&other.mut_vars).cloned().collect(),
        }
    }
}

// Get the information computed by this analysis.
pub fn get_info(data: &FunctionData) -> &SpecGlobalVariableInfo {
    data.annotations.get::<SpecGlobalVariableInfo>().unwrap()
}

fn set_info(env: &FunctionEnv, data: &mut FunctionData, info: SpecGlobalVariableInfo) {
    assert!(
        !data.annotations.has::<SpecGlobalVariableInfo>(),
        "spec global variable info already set: function={}",
        env.get_full_name_str(),
    );
    data.annotations.set::<SpecGlobalVariableInfo>(info, true);
}

fn get_callee_fun_type_instances(
    data: &FunctionData,
    callee_id: &QualifiedId<FunId>,
) -> BTreeSet<Vec<Type>> {
    data.code
        .iter()
        .filter_map(|bc| match bc {
            Bytecode::Call(_, _, Operation::Function(module_id, fun_id, type_inst), _, _)
                if &module_id.qualified(*fun_id) == callee_id =>
            {
                Some(type_inst.clone())
            }
            _ => None,
        })
        .collect()
}

fn get_spec_global_instances(fun_target: &FunctionTarget) -> BTreeSet<Vec<Type>> {
    get_callee_fun_type_instances(
        fun_target.data,
        &fun_target.func_env.module_env.env.global_qid(),
    )
}

fn get_spec_declare_global_instances(fun_target: &FunctionTarget) -> BTreeSet<Vec<Type>> {
    get_callee_fun_type_instances(
        fun_target.data,
        &fun_target.func_env.module_env.env.declare_global_qid(),
    )
}

fn get_spec_declare_global_mut_instances(fun_target: &FunctionTarget) -> BTreeSet<Vec<Type>> {
    get_callee_fun_type_instances(
        fun_target.data,
        &fun_target.func_env.module_env.env.declare_global_mut_qid(),
    )
}

pub fn collect_spec_global_variable_info(
    targets: &FunctionTargetsHolder,
    func_env: &FunctionEnv,
    code: &[Bytecode],
) -> SpecGlobalVariableInfo {
    let (imm_iter, mut_iter): (Vec<_>, Vec<_>) = code
        .iter()
        .filter_map(|bc| match bc {
            Bytecode::Call(_, _, Operation::Function(module_id, fun_id, type_inst), _, _) => {
                let callee_id = module_id.qualified(*fun_id);

                if callee_id == func_env.get_qualified_id() {
                    return None;
                }

                if callee_id == func_env.module_env.env.global_qid() {
                    return Some((vec![type_inst.clone()], vec![]));
                }

                let fun_id_with_info = match targets.get_opaque_spec_by_fun(&callee_id) {
                    Some(spec_id) => {
                        if spec_id != &func_env.get_qualified_id() {
                            spec_id
                        } else {
                            &callee_id
                        }
                    }
                    None => &callee_id,
                };

                if func_env
                    .module_env
                    .env
                    .get_function(*fun_id_with_info)
                    .is_native_or_intrinsic()
                {
                    return None;
                }

                let info = get_info(
                    targets
                        .get_data(fun_id_with_info, &FunctionVariant::Baseline)
                        .unwrap(),
                );
                Some((
                    info.imm_vars()
                        .iter()
                        .map(|tys| tys.iter().map(|ty| ty.instantiate(type_inst)).collect_vec())
                        .collect_vec(),
                    info.mut_vars()
                        .iter()
                        .map(|tys| tys.iter().map(|ty| ty.instantiate(type_inst)).collect_vec())
                        .collect_vec(),
                ))
            }
            _ => None,
        })
        .unzip();

    SpecGlobalVariableInfo {
        imm_vars: imm_iter.into_iter().flatten().collect(),
        mut_vars: mut_iter.into_iter().flatten().collect(),
    }
}

pub struct SpecGlobalVariableAnalysisProcessor();

impl SpecGlobalVariableAnalysisProcessor {
    pub fn new() -> Box<Self> {
        Box::new(Self())
    }
}

impl FunctionTargetProcessor for SpecGlobalVariableAnalysisProcessor {
    fn process(
        &self,
        targets: &mut FunctionTargetsHolder,
        func_env: &FunctionEnv,
        mut data: FunctionData,
        scc_opt: Option<&[FunctionEnv]>,
    ) -> FunctionData {
        // assert!(scc_opt.is_none(), "recursive functions not supported");

        let info = collect_spec_global_variable_info(targets, func_env, &data.code);
        if targets.is_spec(&func_env.get_qualified_id()) {
            let spec_info = get_info(&data);
            let all_vars = spec_info.all_vars().collect();
            let undeclared_imm_vars = info.imm_vars().difference(&all_vars).collect_vec();
            let undeclared_mut_vars = info
                .mut_vars()
                .difference(&spec_info.mut_vars())
                .collect_vec();

            assert!(
                undeclared_imm_vars.is_empty() && undeclared_mut_vars.is_empty(),
                "undeclared spec global variables: function={}, imm_vars=[{}], mut_vars=[{}]",
                func_env.get_full_name_str(),
                undeclared_imm_vars
                    .iter()
                    .map(|tys| format!(
                        "({})",
                        tys.iter()
                            .map(|ty| ty.display(&func_env.get_type_display_ctx()).to_string())
                            .join(", ")
                    ))
                    .join(", "),
                undeclared_mut_vars
                    .iter()
                    .map(|tys| format!(
                        "({})",
                        tys.iter()
                            .map(|ty| ty.display(&func_env.get_type_display_ctx()).to_string())
                            .join(", ")
                    ))
                    .join(", "),
            );
            data.code = data
                .code
                .into_iter()
                .filter(|bc| match bc {
                    Bytecode::Call(_, _, Operation::Function(module_id, fun_id, _), _, _) => {
                        let callee_id = module_id.qualified(*fun_id);
                        return callee_id != func_env.module_env.env.declare_global_qid()
                            && callee_id != func_env.module_env.env.declare_global_mut_qid();
                    }
                    _ => true,
                })
                .collect();
        } else {
            set_info(func_env, &mut data, info);
        }

        data
    }

    fn initialize(&self, env: &GlobalEnv, targets: &mut FunctionTargetsHolder) {
        let spec_ids = targets.specs().map(|id| *id).collect_vec();
        for spec_id in spec_ids {
            let spec_env = env.get_function(spec_id);
            let spec_data = targets
                .get_data_mut(&spec_id, &FunctionVariant::Baseline)
                .unwrap();
            let info = SpecGlobalVariableInfo {
                imm_vars: get_spec_declare_global_instances(&FunctionTarget::new(
                    &spec_env, spec_data,
                )),
                mut_vars: get_spec_declare_global_mut_instances(&FunctionTarget::new(
                    &spec_env, spec_data,
                )),
            };

            for tys in get_spec_global_instances(&FunctionTarget::new(&spec_env, spec_data)) {
                assert!(
                    info.imm_vars().contains(&tys) || info.mut_vars().contains(&tys),
                    "undeclared spec global variable: function={}, tys=({})",
                    spec_env.get_full_name_str(),
                    tys.iter()
                        .map(|ty| ty.display(&spec_env.get_type_display_ctx()).to_string())
                        .join(", "),
                );
            }

            set_info(&spec_env, spec_data, info);
        }
    }

    fn name(&self) -> String {
        "spec_global_variable_analysis".to_string()
    }
}
