use itertools::Itertools;
use std::collections::{BTreeMap, BTreeSet};

use codespan_reporting::diagnostic::{Diagnostic, Label, Severity};

use move_model::{
    model::{FunId, FunctionEnv, GlobalEnv, Loc, QualifiedId},
    ty::Type,
};

use crate::{
    function_target::{self, FunctionData, FunctionTarget},
    function_target_pipeline::{FunctionTargetProcessor, FunctionTargetsHolder, FunctionVariant},
    stackless_bytecode::{Bytecode, Operation},
};

/// The environment extension computed by this analysis.
#[derive(Clone, Debug)]
pub struct SpecGlobalVariableInfo {
    imm_vars: BTreeSet<Vec<Type>>,
    mut_vars: BTreeSet<Vec<Type>>,
    imm_vars_locs: BTreeMap<Vec<Type>, BTreeSet<Loc>>,
    mut_vars_locs: BTreeMap<Vec<Type>, BTreeSet<Loc>>,
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
        let mut imm_vars_locs = self.imm_vars_locs.clone();
        for (other_vars, other_locs) in other.imm_vars_locs.iter() {
            imm_vars_locs
                .entry(other_vars.clone())
                .or_insert(BTreeSet::new())
                .extend(other_locs.clone());
        }

        let mut mut_vars_locs = self.mut_vars_locs.clone();
        for (other_vars, other_locs) in other.mut_vars_locs.iter() {
            mut_vars_locs
                .entry(other_vars.clone())
                .or_insert(BTreeSet::new())
                .extend(other_locs.clone());
        }
        Self {
            imm_vars: self.imm_vars.union(&other.imm_vars).cloned().collect(),
            mut_vars: self.mut_vars.union(&other.mut_vars).cloned().collect(),
            imm_vars_locs,
            mut_vars_locs,
        }
    }

    pub fn instantiate(&self, type_inst: &[Type]) -> Self {
        Self {
            imm_vars: self
                .imm_vars
                .iter()
                .map(|tys| tys.iter().map(|ty| ty.instantiate(type_inst)).collect_vec())
                .collect(),
            mut_vars: self
                .mut_vars
                .iter()
                .map(|tys| tys.iter().map(|ty| ty.instantiate(type_inst)).collect_vec())
                .collect(),
            imm_vars_locs: self
                .imm_vars_locs
                .iter()
                .map(|(tys, locs)| {
                    (
                        tys.iter().map(|ty| ty.instantiate(type_inst)).collect(),
                        locs.clone(),
                    )
                })
                .collect(),
            mut_vars_locs: self
                .mut_vars_locs
                .iter()
                .map(|(tys, locs)| {
                    (
                        tys.iter().map(|ty| ty.instantiate(type_inst)).collect(),
                        locs.clone(),
                    )
                })
                .collect(),
        }
    }

    pub fn singleton_imm(type_inst: &[Type], loc: &Loc) -> Self {
        Self {
            imm_vars: BTreeSet::from([type_inst.to_vec()]),
            mut_vars: BTreeSet::new(),
            imm_vars_locs: BTreeMap::from([(type_inst.to_vec(), BTreeSet::from([loc.clone()]))]),
            mut_vars_locs: BTreeMap::new(),
        }
    }

    pub fn singleton_mut(type_inst: &[Type], loc: &Loc) -> Self {
        Self {
            imm_vars: BTreeSet::new(),
            mut_vars: BTreeSet::from([type_inst.to_vec()]),
            imm_vars_locs: BTreeMap::new(),
            mut_vars_locs: BTreeMap::from([(type_inst.to_vec(), BTreeSet::from([loc.clone()]))]),
        }
    }

    pub fn info_union(info: impl Iterator<Item = Self>) -> Self {
        info.fold(
            Self {
                imm_vars: BTreeSet::new(),
                mut_vars: BTreeSet::new(),
                imm_vars_locs: BTreeMap::new(),
                mut_vars_locs: BTreeMap::new(),
            },
            |acc, info| acc.union(&info),
        )
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

pub fn collect_spec_global_variable_info(
    targets: &FunctionTargetsHolder,
    fun_target: &FunctionTarget,
    code: &[Bytecode],
) -> SpecGlobalVariableInfo {
    let infos_iter = code.iter().filter_map(|bc| match bc {
        Bytecode::Call(_, _, Operation::Function(module_id, fun_id, type_inst), _, _) => {
            let callee_id = module_id.qualified(*fun_id);
            let loc = fun_target.get_bytecode_loc(bc.get_attr_id());

            if callee_id == fun_target.func_env.get_qualified_id() {
                return None;
            }

            if callee_id == fun_target.func_env.module_env.env.global_qid() {
                return Some(SpecGlobalVariableInfo::singleton_imm(type_inst, &loc));
            }

            if callee_id == fun_target.func_env.module_env.env.log_ghost_qid() {
                return Some(SpecGlobalVariableInfo::singleton_imm(type_inst, &loc));
            }

            let fun_id_with_info = match targets.get_spec_by_fun(&callee_id) {
                Some(spec_id) => {
                    if spec_id != &fun_target.func_env.get_qualified_id() {
                        spec_id
                    } else {
                        &callee_id
                    }
                }
                None => &callee_id,
            };

            // native or intrinsic functions are without specs do not have spec global variables
            if fun_target
                .func_env
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
            Some(info.instantiate(type_inst))
        }
        _ => None,
    });
    SpecGlobalVariableInfo::info_union(infos_iter)
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

        let info = collect_spec_global_variable_info(
            targets,
            &FunctionTarget::new(func_env, &data),
            &data.code,
        );

        if targets.is_spec(&func_env.get_qualified_id()) {
            let spec_info = get_info(&data);
            let all_vars = spec_info.all_vars().collect();
            let undeclared_imm_vars = info.imm_vars().difference(&all_vars).collect_vec();
            let undeclared_mut_vars = info
                .mut_vars()
                .difference(&spec_info.mut_vars())
                .collect_vec();

            for var in undeclared_imm_vars {
                let primary_labels = info
                    .imm_vars_locs
                    .get(var)
                    .unwrap()
                    .iter()
                    .map(|loc| Label::primary(loc.file_id(), loc.span()))
                    .collect();
                let secondary_label =
                    Label::secondary(func_env.get_loc().file_id(), func_env.get_loc().span());
                let diag = Diagnostic::new(Severity::Error)
                    .with_code("E0011")
                    .with_message(&format!(
                        "undeclared immutable global variable {}:",
                        var[0].display(&func_env.get_type_display_ctx()).to_string()
                    ))
                    .with_labels(primary_labels)
                    .with_labels(vec![secondary_label]);
                func_env.module_env.env.add_diag(diag);
            }

            for var in undeclared_mut_vars {
                let primary_labels = info
                    .mut_vars_locs
                    .get(var)
                    .unwrap()
                    .iter()
                    .map(|loc| Label::primary(loc.file_id(), loc.span()))
                    .collect();
                let secondary_label =
                    Label::secondary(func_env.get_loc().file_id(), func_env.get_loc().span());
                let diag = Diagnostic::new(Severity::Error)
                    .with_code("E0012")
                    .with_message(&format!(
                        "undeclared mutable global variable {}:",
                        var[0].display(&func_env.get_type_display_ctx()).to_string()
                    ))
                    .with_labels(primary_labels)
                    .with_labels(vec![secondary_label]);
                func_env.module_env.env.add_diag(diag);
            }

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
            let spec_target = FunctionTarget::new(&spec_env, spec_data);

            let infos_iter = spec_data.code.iter().filter_map(|bc| match bc {
                Bytecode::Call(_, _, Operation::Function(module_id, fun_id, type_inst), _, _) => {
                    let callee_id = module_id.qualified(*fun_id);
                    let loc = spec_target.get_bytecode_loc(bc.get_attr_id());

                    if callee_id == spec_target.func_env.module_env.env.declare_global_qid() {
                        return Some(SpecGlobalVariableInfo::singleton_imm(type_inst, &loc));
                    }

                    if callee_id == spec_target.func_env.module_env.env.declare_global_mut_qid() {
                        return Some(SpecGlobalVariableInfo::singleton_mut(type_inst, &loc));
                    }

                    None
                }
                _ => None,
            });
            let info = SpecGlobalVariableInfo::info_union(infos_iter);

            set_info(&spec_env, spec_data, info);
        }
    }

    fn name(&self) -> String {
        "spec_global_variable_analysis".to_string()
    }
}
