use core::fmt;
use std::collections::{BTreeMap, BTreeSet};

use codespan_reporting::diagnostic::{Diagnostic, Label, Severity};
use itertools::Itertools;

use move_model::{
    model::{FunctionEnv, GlobalEnv, Loc},
    ty::Type,
};

use crate::{
    function_target::{FunctionData, FunctionTarget},
    function_target_pipeline::{FunctionTargetProcessor, FunctionTargetsHolder, FunctionVariant},
    stackless_bytecode::{Bytecode, Operation},
};

/// The environment extension computed by this analysis.
#[derive(Clone, Debug, PartialEq, Eq)]
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

    pub fn all_vars(&self) -> impl Iterator<Item = &Vec<Type>> + '_ {
        self.imm_vars.union(&self.mut_vars)
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

    pub fn instantiate(
        &self,
        type_inst: &[Type],
    ) -> Result<Self, BTreeMap<Vec<Type>, (BTreeSet<Vec<Type>>, BTreeSet<Vec<Type>>)>> {
        let inst_mut_vars = self
            .mut_vars
            .iter()
            .map(|tys| {
                (
                    tys.iter().map(|ty| ty.instantiate(type_inst)).collect_vec(),
                    tys.clone(),
                )
            })
            .fold(BTreeMap::new(), |mut map, (key, val)| {
                map.entry(key).or_insert_with(BTreeSet::new).insert(val);
                map
            });
        let inst_imm_vars = self
            .imm_vars
            .iter()
            .map(|tys| {
                (
                    tys.iter().map(|ty| ty.instantiate(type_inst)).collect_vec(),
                    tys.clone(),
                )
            })
            .fold(BTreeMap::new(), |mut map, (key, val)| {
                map.entry(key).or_insert_with(BTreeSet::new).insert(val);
                map
            });
        let conflicts = inst_mut_vars
            .into_iter()
            .map(|(key, val)| {
                (
                    key.clone(),
                    (
                        val.clone(),
                        inst_imm_vars
                            .get(&key)
                            .map(|x| x.clone())
                            .unwrap_or_else(BTreeSet::new),
                    ),
                )
            })
            .filter(|(_key, (mut_var_set, imm_var_set))| {
                mut_var_set.len() > 1 || imm_var_set.len() > 0
            })
            .collect::<BTreeMap<Vec<Type>, (BTreeSet<Vec<Type>>, BTreeSet<Vec<Type>>)>>();
        if !conflicts.is_empty() {
            return Err(conflicts);
        }

        Ok(Self {
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
                .fold(BTreeMap::new(), |mut map, (key, val)| {
                    map.entry(key).or_insert_with(BTreeSet::new).extend(val);
                    map
                }),
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
        })
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

            if callee_id == fun_target.func_env.module_env.env.global_set_qid() {
                return Some(SpecGlobalVariableInfo::singleton_mut(type_inst, &loc));
            }

            if callee_id == fun_target.func_env.module_env.env.global_borrow_mut_qid() {
                return Some(SpecGlobalVariableInfo::singleton_mut(type_inst, &loc));
            }

            if callee_id == fun_target.func_env.module_env.env.log_ghost_qid() {
                return Some(SpecGlobalVariableInfo::singleton_imm(type_inst, &loc));
            }

            let fun_id_with_info = targets
                .get_callee_spec_qid(&fun_target.func_env.get_qualified_id(), &callee_id)
                .unwrap_or(&callee_id);

            // native or intrinsic functions are without specs do not have spec global variables
            if fun_target
                .func_env
                .module_env
                .env
                .get_function(*fun_id_with_info)
                .is_native()
            {
                return None;
            }

            let info = get_info(
                targets
                    .get_data(fun_id_with_info, &FunctionVariant::Baseline)
                    .unwrap(),
            );
            match info.instantiate(type_inst) {
                Ok(inst_info) => Some(inst_info),
                Err(conflicts) => {
                    for (tys, (mut_vars, imm_vars)) in conflicts {
                        fun_target.func_env.module_env.env.add_diag(
                            Diagnostic::new(Severity::Error)
                                .with_code("E0015")
                                .with_message(&format!(
                                    "global variable instantiation conflict {}:",
                                    tys[0]
                                        .display(&fun_target.func_env.get_named_type_display_ctx())
                                        .to_string()
                                ))
                                .with_labels(vec![Label::primary(loc.file_id(), loc.span())])
                                .with_labels(
                                    mut_vars
                                        .iter()
                                        .flat_map(|var| {
                                            info.mut_vars_locs.get(var).unwrap().iter().map(|loc| {
                                                Label::secondary(loc.file_id(), loc.span())
                                            })
                                        })
                                        .collect(),
                                )
                                .with_labels(
                                    imm_vars
                                        .iter()
                                        .flat_map(|var| {
                                            info.imm_vars_locs.get(var).unwrap().iter().map(|loc| {
                                                Label::secondary(loc.file_id(), loc.span())
                                            })
                                        })
                                        .collect(),
                                ),
                        );
                    }

                    None
                }
            }
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
        let info = collect_spec_global_variable_info(
            targets,
            &FunctionTarget::new(func_env, &data),
            &data.code,
        );

        if targets.is_spec(&func_env.get_qualified_id()) {
            let spec_info = get_info(&data);

            let spec_vars = spec_info
                .all_vars()
                .map(|var| (var[0].clone(), var[1].clone()))
                .collect::<BTreeMap<_, _>>();
            for var in info.all_vars() {
                let var_name = &var[0];
                let var_ty = &var[1];

                if spec_info.all_vars().contains(&var) {
                    // check mutability
                    if info.mut_vars().contains(var) && !spec_info.mut_vars().contains(var) {
                        // if the variable is declared as immutable in spec but used as mutable
                        let primary_labels = info
                            .mut_vars_locs
                            .get(var)
                            .unwrap()
                            .iter()
                            .map(|loc| Label::primary(loc.file_id(), loc.span()))
                            .collect();
                        let spec_var_loc =
                            spec_info.imm_vars_locs.get(var).unwrap().first().unwrap();
                        let secondary_label =
                            Label::secondary(spec_var_loc.file_id(), spec_var_loc.span());
                        let diag = Diagnostic::new(Severity::Error)
                            .with_code("E0013")
                            .with_message(&format!(
                                "immutable global variable {} used as mutable:",
                                var_name
                                    .display(&func_env.get_named_type_display_ctx())
                                    .to_string()
                            ))
                            .with_labels(primary_labels)
                            .with_labels(vec![secondary_label]);
                        func_env.module_env.env.add_diag(diag);
                    }
                    continue;
                }

                let imm_locs = info.imm_vars_locs.get(var).into_iter().flatten();
                let mut_locs = info.mut_vars_locs.get(var).into_iter().flatten();
                let all_locs = imm_locs.chain(mut_locs).collect::<BTreeSet<_>>();

                if let Some(spec_var_ty) = spec_vars.get(var_name) {
                    // type mismatch
                    let primary_labels = all_locs
                        .iter()
                        .map(|loc| Label::primary(loc.file_id(), loc.span()))
                        .collect();
                    let spec_var = vec![var_name.clone(), spec_var_ty.clone()];
                    let spec_var_loc = spec_info
                        .imm_vars_locs
                        .get(&spec_var)
                        .unwrap_or_else(|| spec_info.mut_vars_locs.get(&spec_var).unwrap())
                        .first()
                        .unwrap();
                    let secondary_label =
                        Label::secondary(spec_var_loc.file_id(), spec_var_loc.span());
                    let diag = Diagnostic::new(Severity::Error)
                        .with_code("E0012")
                        .with_message(&format!(
                            "type mismatch for global variable {}: expected {}, found {}",
                            var_name.display(&func_env.get_named_type_display_ctx()),
                            spec_var_ty.display(&func_env.get_named_type_display_ctx()),
                            var_ty.display(&func_env.get_named_type_display_ctx())
                        ))
                        .with_labels(primary_labels)
                        .with_labels(vec![secondary_label]);
                    func_env.module_env.env.add_diag(diag);
                    continue;
                }

                let primary_labels = all_locs
                    .iter()
                    .map(|loc| Label::primary(loc.file_id(), loc.span()))
                    .collect();
                let secondary_label =
                    Label::secondary(func_env.get_loc().file_id(), func_env.get_loc().span());
                let diag = Diagnostic::new(Severity::Error)
                    .with_code("E0011")
                    .with_message(&format!(
                        "undeclared global variable {}:",
                        var_name
                            .display(&func_env.get_named_type_display_ctx())
                            .to_string()
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
            for bc in &data.code {
                if let Bytecode::Call(attr_id, _, Operation::Function(module_id, fun_id, _), _, _) =
                    bc
                {
                    let callee_id = module_id.qualified(*fun_id);
                    if callee_id == func_env.module_env.env.declare_global_qid()
                        || callee_id == func_env.module_env.env.declare_global_mut_qid()
                    {
                        let loc = FunctionTarget::new(func_env, &data).get_bytecode_loc(*attr_id);
                        let diag = Diagnostic::new(Severity::Error)
                            .with_code("E0014")
                            .with_message(
                                "unexpected ghost variable declaration. Declare ghost variables in #[spec] functions.",
                            )
                            .with_labels(vec![Label::primary(loc.file_id(), loc.span())]);
                        func_env.module_env.env.add_diag(diag);
                    }
                }
            }

            data.annotations
                .set_with_fixedpoint_check(info, scc_opt.is_some());
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

            spec_data
                .annotations
                .set::<SpecGlobalVariableInfo>(info, true);
        }
    }

    fn dump_result(
        &self,
        f: &mut fmt::Formatter,
        env: &GlobalEnv,
        targets: &FunctionTargetsHolder,
    ) -> fmt::Result {
        writeln!(f, "\n\n==== spec global variable analysis summaries ====\n")?;
        for ref module in env.get_modules() {
            for ref fun in module.get_functions() {
                for (_, ref target) in targets.get_targets(fun) {
                    let info = get_info(target.data);
                    writeln!(f, "fun {}", fun.get_full_name_str())?;
                    for var in info.mut_vars() {
                        writeln!(
                            f,
                            "  mutable {}",
                            var[0].display(&fun.get_named_type_display_ctx())
                        )?;
                    }
                    for var in info.imm_vars() {
                        writeln!(
                            f,
                            "  immutable {}",
                            var[0].display(&fun.get_named_type_display_ctx())
                        )?;
                    }
                }
            }
        }
        Ok(())
    }

    fn name(&self) -> String {
        "spec_global_variable_analysis".to_string()
    }
}
