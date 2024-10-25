// Copyright (c) The Diem Core Contributors
// Copyright (c) The Move Contributors
// SPDX-License-Identifier: Apache-2.0

//! Analysis which computes information needed in backends for monomorphization. This
//! computes the distinct type instantiations in the model for structs and inlined functions.

use std::{
    collections::{BTreeMap, BTreeSet},
    fmt,
    rc::Rc,
};

use itertools::Itertools;

use move_model::{
    ast,
    ast::{Condition, ConditionKind, ExpData},
    model::{
        DatatypeId, FunId, GlobalEnv, ModuleId, QualifiedId, QualifiedInstId, SpecFunId, SpecVarId,
        StructEnv,
    },
    pragmas::INTRINSIC_TYPE_MAP,
    ty::{Type, TypeDisplayContext, TypeInstantiationDerivation, TypeUnificationAdapter, Variance},
    well_known::{
        TYPE_INFO_MOVE, TYPE_INFO_SPEC, TYPE_NAME_GET_MOVE, TYPE_NAME_GET_SPEC, TYPE_NAME_MOVE,
        TYPE_NAME_SPEC, TYPE_SPEC_IS_STRUCT,
    },
};

use crate::{
    function_target::FunctionTarget,
    function_target_pipeline::{FunctionTargetProcessor, FunctionTargetsHolder, FunctionVariant},
    spec_global_variable_analysis,
    stackless_bytecode::{BorrowEdge, Bytecode, Operation},
    usage_analysis::UsageProcessor,
    verification_analysis,
};

/// The environment extension computed by this analysis.
#[derive(Clone, Default, Debug)]
pub struct MonoInfo {
    pub structs: BTreeMap<QualifiedId<DatatypeId>, BTreeSet<Vec<Type>>>,
    pub funs: BTreeMap<(QualifiedId<FunId>, FunctionVariant), BTreeSet<Vec<Type>>>,
    pub spec_funs: BTreeMap<QualifiedId<SpecFunId>, BTreeSet<Vec<Type>>>,
    pub spec_vars: BTreeMap<QualifiedId<SpecVarId>, BTreeSet<Vec<Type>>>,
    pub type_params: BTreeSet<u16>,
    pub vec_inst: BTreeSet<Type>,
    pub table_inst: BTreeMap<QualifiedId<DatatypeId>, BTreeSet<(Type, Type)>>,
    pub native_inst: BTreeMap<ModuleId, BTreeSet<Vec<Type>>>,
    pub all_types: BTreeSet<Type>,
    pub axioms: Vec<(Condition, Vec<Vec<Type>>)>,
}

impl MonoInfo {
    pub fn dump(&self, env: &GlobalEnv, f: &mut fmt::Formatter) -> fmt::Result {
        writeln!(f, "\n\n==== mono-analysis result ====\n")?;
        let tctx = TypeDisplayContext::WithEnv {
            env,
            type_param_names: None,
        };
        let display_inst = |tys: &[Type]| {
            tys.iter()
                .map(|ty| ty.display(&tctx).to_string())
                .join(", ")
        };
        for param_idx in &self.type_params {
            writeln!(
                f,
                "type parameter {}",
                Type::TypeParameter(*param_idx).display(&tctx)
            )?;
        }
        for (sid, insts) in &self.structs {
            let sname = env.get_struct(*sid).get_full_name_str();
            writeln!(f, "struct {} = {{", sname)?;
            for inst in insts {
                writeln!(f, "  <{}>", display_inst(inst))?;
            }
            writeln!(f, "}}")?;
        }
        for ((fid, variant), insts) in &self.funs {
            let fname = env.get_function(*fid).get_full_name_str();
            writeln!(f, "fun {} [{}] = {{", fname, variant)?;
            for inst in insts {
                writeln!(f, "  <{}>", display_inst(inst))?;
            }
            writeln!(f, "}}")?;
        }
        for (fid, insts) in &self.spec_funs {
            let module_env = env.get_module(fid.module_id);
            let decl = module_env.get_spec_fun(fid.id);
            let mname = module_env.get_full_name_str();
            let fname = decl.name.display(env.symbol_pool());
            writeln!(f, "spec fun {}::{} = {{", mname, fname)?;
            for inst in insts {
                writeln!(f, "  <{}>", display_inst(inst))?;
            }
            writeln!(f, "}}")?;
        }
        for (module, insts) in &self.native_inst {
            writeln!(
                f,
                "module {} = {{",
                env.get_module(*module).get_full_name_str()
            )?;
            for inst in insts {
                writeln!(f, "  <{}>", display_inst(inst))?;
            }
            writeln!(f, "}}")?;
        }
        for (cond, insts) in &self.axioms {
            writeln!(f, "axiom {} = {{", cond.loc.display(env))?;
            for inst in insts {
                writeln!(f, "  <{}>", display_inst(inst))?;
            }
            writeln!(f, "}}")?;
        }

        Ok(())
    }

    pub fn dump_cfg(&self, env: &GlobalEnv, f: &mut fmt::Formatter) -> fmt::Result {
        writeln!(f, "=== verification call graph ===")?;
        for (func_id, variant) in self.funs.keys() {
            let func_env = env.get_function(*func_id);
            writeln!(
                f,
                "fun {} [{}] -> {{",
                func_env.get_full_name_str(),
                variant
            )?;
            for callee_id in func_env.get_called_functions() {
                let callee_env = env.get_function(callee_id);
                writeln!(f, "  {}", callee_env.get_full_name_str())?;
            }
            writeln!(f, "}}")?;
        }
        Ok(())
    }
}

pub struct MonoInfoCFGDisplay<'a> {
    pub info: &'a MonoInfo,
    pub env: &'a GlobalEnv,
}

impl<'a> fmt::Display for MonoInfoCFGDisplay<'a> {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        self.info.dump_cfg(self.env, f)
    }
}

/// Get the information computed by this analysis.
pub fn get_info(env: &GlobalEnv) -> Rc<MonoInfo> {
    env.get_extension::<MonoInfo>().unwrap()
}

pub struct MonoAnalysisProcessor();

impl MonoAnalysisProcessor {
    pub fn new() -> Box<Self> {
        Box::new(Self())
    }
}

/// This processor computes monomorphization information for backends.
impl FunctionTargetProcessor for MonoAnalysisProcessor {
    fn name(&self) -> String {
        "mono_analysis".to_owned()
    }

    fn is_single_run(&self) -> bool {
        true
    }

    fn run(&self, env: &GlobalEnv, targets: &mut FunctionTargetsHolder) {
        self.analyze(env, targets);
    }

    fn dump_result(
        &self,
        f: &mut fmt::Formatter,
        env: &GlobalEnv,
        _targets: &FunctionTargetsHolder,
    ) -> fmt::Result {
        let info = env
            .get_extension::<MonoInfo>()
            .expect("monomorphization analysis not run");
        info.dump(env, f)
    }
}

// Instantiation Analysis
// ======================

impl MonoAnalysisProcessor {
    fn analyze<'a>(&self, env: &'a GlobalEnv, targets: &'a FunctionTargetsHolder) {
        let mut analyzer = Analyzer {
            env,
            targets,
            info: MonoInfo::default(),
            todo_funs: vec![],
            done_funs: BTreeSet::new(),
            todo_spec_funs: vec![],
            done_spec_funs: BTreeSet::new(),
            done_types: BTreeSet::new(),
            inst_opt: None,
        };
        // Analyze axioms found in modules.
        for module_env in env.get_modules() {
            for axiom in module_env.get_spec().filter_kind_axiom() {
                analyzer.analyze_exp(&axiom.exp)
            }
        }
        // Analyze functions
        analyzer.analyze_funs();
        let Analyzer {
            mut info,
            done_types,
            ..
        } = analyzer;
        info.all_types = done_types;
        env.set_extension(info);
    }
}

struct Analyzer<'a> {
    env: &'a GlobalEnv,
    targets: &'a FunctionTargetsHolder,
    info: MonoInfo,
    todo_funs: Vec<(QualifiedId<FunId>, FunctionVariant, Vec<Type>)>,
    done_funs: BTreeSet<(QualifiedId<FunId>, FunctionVariant, Vec<Type>)>,
    todo_spec_funs: Vec<(QualifiedId<SpecFunId>, Vec<Type>)>,
    done_spec_funs: BTreeSet<(QualifiedId<SpecFunId>, Vec<Type>)>,
    done_types: BTreeSet<Type>,
    inst_opt: Option<Vec<Type>>,
}

impl<'a> Analyzer<'a> {
    fn analyze_funs(&mut self) {
        // Analyze top-level, verified functions. Any functions they call will be queued
        // in self.todo_targets for later analysis. During this phase, self.inst_opt is None.
        for module in self.env.get_modules() {
            for fun in module.get_functions() {
                for (variant, target) in self.targets.get_targets(&fun) {
                    if !(variant.is_verified()
                        || self.targets.is_spec(&fun.get_qualified_id())
                            && verification_analysis::get_info(&target).inlined)
                    {
                        continue;
                    }

                    self.analyze_fun(target.clone());

                    let info = spec_global_variable_analysis::get_info(&target.data);
                    for tys in info.all_vars() {
                        for ty in &tys {
                            self.add_type_root(ty)
                        }
                    }

                    // We also need to analyze all modify targets because they are not
                    // included in the bytecode.
                    for (_, exps) in target.get_modify_ids_and_exps() {
                        for exp in exps {
                            self.analyze_exp(exp);
                        }
                    }
                }
            }
        }

        // Next do todo-list for regular functions, while self.inst_opt contains the
        // specific instantiation.
        while let Some((fun, variant, inst)) = self.todo_funs.pop() {
            self.inst_opt = Some(inst);
            self.analyze_fun(
                self.targets
                    .get_target(&self.env.get_function(fun), &variant),
            );
            let inst = std::mem::take(&mut self.inst_opt).unwrap();
            // Insert it into final analysis result.
            self.info
                .funs
                .entry((fun, variant))
                .or_default()
                .insert(inst);
        }

        // Next do axioms, based on the types discovered for regular functions.
        let axioms = self.compute_axiom_instances();
        for (cond, insts) in axioms {
            for inst in &insts {
                self.inst_opt = Some(inst.clone());
                self.analyze_exp(&cond.exp);
            }
            self.info.axioms.push((cond, insts))
        }

        // Finally do spec functions, after all regular functions and axioms are done.
        while let Some((fun, inst)) = self.todo_spec_funs.pop() {
            self.inst_opt = Some(inst);
            self.analyze_spec_fun(fun);
            let inst = std::mem::take(&mut self.inst_opt).unwrap();
            // Insert it into final analysis result.
            self.info
                .spec_funs
                .entry(fun)
                .or_default()
                .insert(inst.clone());
            self.done_spec_funs.insert((fun, inst));
        }
    }

    /// Analyze axioms, computing all the instantiations needed. We over-approximate the
    /// instantiations by using the cartesian product of all known types. As the number of
    /// type parameters for axioms is restricted to 2, the number of instantiations
    /// should stay in range. Since each axiom instance is eventually instantiated for
    /// distinct types, unnecessary axioms should be ignorable by the SMT solver, avoiding
    /// over-triggering.
    fn compute_axiom_instances(&self) -> Vec<(Condition, Vec<Vec<Type>>)> {
        let mut axioms = vec![];
        let all_types = self
            .done_types
            .iter()
            .filter(|t| t.can_be_type_argument())
            .cloned()
            .collect::<Vec<_>>();
        for module_env in self.env.get_modules() {
            for cond in &module_env.get_spec().conditions {
                if let ConditionKind::Axiom(params) = &cond.kind {
                    let type_insts = match params.len() {
                        0 => vec![vec![]],
                        1 => all_types.iter().cloned().map(|t| vec![t]).collect(),
                        2 => itertools::iproduct!(
                            all_types.iter().cloned(),
                            all_types.iter().cloned()
                        )
                        .map(|(x, y)| vec![x, y])
                        .collect(),
                        _ => {
                            self.env.error(
                                &cond.loc,
                                "axioms cannot have more than two type parameters",
                            );
                            vec![]
                        }
                    };
                    axioms.push((cond.clone(), type_insts));
                }
            }
        }
        axioms
    }

    fn analyze_fun(&mut self, target: FunctionTarget<'_>) {
        self.analyze_fun_types(&target, self.inst_opt.clone());
        // Analyze code.
        if !target.func_env.is_native_or_intrinsic() {
            for bc in target.get_bytecode() {
                self.analyze_bytecode(&target, bc);
            }
        }

        // Analyze instantiations (when this function is a verification target)
        if self.inst_opt.is_none() {
            // collect information
            let fun_type_params_arity = target.get_type_parameter_count();
            let usage_state = UsageProcessor::analyze(self.targets, target.func_env, target.data);

            // collect instantiations
            let mut all_insts = BTreeSet::new();
            for lhs_m in usage_state.accessed.all.iter() {
                let lhs_ty = lhs_m.to_type();
                for rhs_m in usage_state.accessed.all.iter() {
                    let rhs_ty = rhs_m.to_type();

                    // make sure these two types unify before trying to instantiate them
                    let adapter = TypeUnificationAdapter::new_pair(&lhs_ty, &rhs_ty, true, true);
                    if adapter.unify(Variance::Allow, false).is_none() {
                        continue;
                    }

                    // find all instantiation combinations given by this unification
                    let fun_insts = TypeInstantiationDerivation::progressive_instantiation(
                        std::iter::once(&lhs_ty),
                        std::iter::once(&rhs_ty),
                        true,
                        false,
                        true,
                        false,
                        fun_type_params_arity,
                        true,
                        false,
                    );
                    all_insts.extend(fun_insts);
                }
            }

            // mark all the instantiated targets as todo
            for fun_inst in all_insts {
                self.done_funs.insert((
                    target.func_env.get_qualified_id(),
                    target.data.variant.clone(),
                    fun_inst.clone(),
                ));
                self.todo_funs.push((
                    target.func_env.get_qualified_id(),
                    target.data.variant.clone(),
                    fun_inst,
                ));
            }
        }
    }

    fn analyze_fun_types(&mut self, target: &FunctionTarget<'_>, inst_opt: Option<Vec<Type>>) {
        let old_inst = std::mem::replace(&mut self.inst_opt, inst_opt);
        // Analyze function locals and return value types.
        for ty in target.func_env.get_parameter_types() {
            self.add_type_root(&ty);
        }
        for idx in target.get_non_parameter_locals() {
            self.add_type_root(target.get_local_type(idx));
        }
        for ty in target.get_return_types().iter() {
            self.add_type_root(ty);
        }
        self.inst_opt = old_inst;
    }

    fn analyze_bytecode(&mut self, target: &FunctionTarget<'_>, bc: &Bytecode) {
        use Bytecode::*;
        use Operation::*;
        // We only need to analyze function calls, not `pack` or other instructions
        // because the types those are using are reflected in locals which are analyzed
        // elsewhere.
        match bc {
            Call(_, _, Function(mid, fid, targs), ..) => {
                let module_env = &self.env.get_module(*mid);
                let callee_env = module_env.get_function(*fid);
                let actuals = self.instantiate_vec(targs);

                // the type reflection functions are specially handled here
                if self.env.get_extlib_address() == *module_env.get_name().addr() {
                    let qualified_name = format!(
                        "{}::{}",
                        module_env.get_name().name().display(self.env.symbol_pool()),
                        callee_env.get_name().display(self.env.symbol_pool()),
                    );
                    if qualified_name == TYPE_NAME_MOVE || qualified_name == TYPE_INFO_MOVE {
                        self.add_type(&actuals[0]);
                    }
                }
                if self.env.get_stdlib_address() == *module_env.get_name().addr() {
                    let qualified_name = format!(
                        "{}::{}",
                        module_env.get_name().name().display(self.env.symbol_pool()),
                        callee_env.get_name().display(self.env.symbol_pool()),
                    );
                    if qualified_name == TYPE_NAME_GET_MOVE {
                        self.add_type(&actuals[0]);
                    }
                }

                if let Some(spec_qid) = self
                    .targets
                    .get_opaque_spec_by_fun(&callee_env.get_qualified_id())
                {
                    self.push_todo_fun(spec_qid.clone(), actuals.clone());
                    if spec_qid == &target.func_env.get_qualified_id()
                        && !self.targets.no_verify_specs().contains(spec_qid)
                    {
                        self.push_todo_fun(callee_env.get_qualified_id(), actuals.clone());
                    } else {
                        self.info
                            .funs
                            .entry((callee_env.get_qualified_id(), FunctionVariant::Baseline))
                            .or_default()
                            .insert(actuals.clone());
                        self.analyze_fun_types(
                            &self
                                .targets
                                .get_target(&callee_env, &FunctionVariant::Baseline),
                            Some(actuals.clone()),
                        );
                    }
                };

                if callee_env.is_native_or_intrinsic() && !actuals.is_empty() {
                    self.info
                        .funs
                        .entry((callee_env.get_qualified_id(), FunctionVariant::Baseline))
                        .or_default()
                        .insert(actuals.clone());
                    self.analyze_fun_types(
                        &self
                            .targets
                            .get_target(&callee_env, &FunctionVariant::Baseline),
                        Some(actuals.clone()),
                    );
                    // Mark the associated module to be instantiated with the given actuals.
                    // This will instantiate all functions in the module with matching number
                    // of type parameters.
                    self.info
                        .native_inst
                        .entry(callee_env.module_env.get_id())
                        .or_default()
                        .insert(actuals);
                } else if !callee_env.is_opaque()
                    && self
                        .targets
                        .get_opaque_spec_by_fun(&callee_env.get_qualified_id())
                        .is_none()
                {
                    // This call needs to be inlined, with targs instantiated by self.inst_opt.
                    // Schedule for later processing if this instance has not been processed yet.
                    self.push_todo_fun(mid.qualified(*fid), actuals);
                }
            }
            Call(_, _, WriteBack(_, edge), ..) => {
                // In very rare occasions, not all types used in the function can appear in
                // function parameters, locals, and return values. Types hidden in the write-back
                // chain of a hyper edge is one such case. Therefore, we need an extra processing
                // to collect types used in borrow edges.
                //
                // TODO(mengxu): need to revisit this once the modeling for dynamic borrow is done
                self.add_types_in_borrow_edge(edge)
            }
            Prop(_, _, exp) => self.analyze_exp(exp),
            SaveMem(_, _, mem) => {
                let mem = self.instantiate_mem(mem.to_owned());
                let struct_env = self.env.get_struct_qid(mem.to_qualified_id());
                self.add_struct(struct_env, &mem.inst);
            }
            _ => {}
        }
    }

    fn push_todo_fun(&mut self, id: QualifiedId<FunId>, actuals: Vec<Type>) {
        let entry = (id, FunctionVariant::Baseline, actuals);
        if !self.done_funs.contains(&entry) {
            self.done_funs.insert(entry.clone());
            self.todo_funs.push(entry);
        }
    }

    fn instantiate_vec(&self, targs: &[Type]) -> Vec<Type> {
        if let Some(inst) = &self.inst_opt {
            Type::instantiate_slice(targs, inst)
        } else {
            targs.to_owned()
        }
    }

    fn instantiate_mem(&self, mem: QualifiedInstId<DatatypeId>) -> QualifiedInstId<DatatypeId> {
        if let Some(inst) = &self.inst_opt {
            mem.instantiate(inst)
        } else {
            mem
        }
    }

    // Expression and Spec Fun Analysis
    // --------------------------------

    fn analyze_spec_fun(&mut self, fun: QualifiedId<SpecFunId>) {
        let module_env = self.env.get_module(fun.module_id);
        let decl = module_env.get_spec_fun(fun.id);
        for (_, ty) in &decl.params {
            self.add_type_root(ty)
        }
        self.add_type_root(&decl.result_type);
        if let Some(exp) = &decl.body {
            self.analyze_exp(exp)
        }
    }

    fn analyze_exp(&mut self, exp: &ExpData) {
        exp.visit(&mut |e| {
            let node_id = e.node_id();
            self.add_type_root(&self.env.get_node_type(node_id));
            for ref ty in self.env.get_node_instantiation(node_id) {
                self.add_type_root(ty);
            }
            if let ExpData::Call(node_id, ast::Operation::Function(mid, fid, _), _) = e {
                let actuals = self.instantiate_vec(&self.env.get_node_instantiation(*node_id));
                let module = self.env.get_module(*mid);
                let spec_fun = module.get_spec_fun(*fid);

                // the type reflection functions are specially handled here
                if self.env.get_extlib_address() == *module.get_name().addr() {
                    let qualified_name = format!(
                        "{}::{}",
                        module.get_name().name().display(self.env.symbol_pool()),
                        spec_fun.name.display(self.env.symbol_pool()),
                    );
                    if qualified_name == TYPE_NAME_SPEC
                        || qualified_name == TYPE_INFO_SPEC
                        || qualified_name == TYPE_SPEC_IS_STRUCT
                    {
                        self.add_type(&actuals[0]);
                    }
                }
                if self.env.get_stdlib_address() == *module.get_name().addr() {
                    let qualified_name = format!(
                        "{}::{}",
                        module.get_name().name().display(self.env.symbol_pool()),
                        spec_fun.name.display(self.env.symbol_pool()),
                    );
                    if qualified_name == TYPE_NAME_GET_SPEC {
                        self.add_type(&actuals[0]);
                    }
                }

                if spec_fun.is_native && !actuals.is_empty() {
                    // Add module to native modules
                    self.info
                        .native_inst
                        .entry(module.get_id())
                        .or_default()
                        .insert(actuals);
                } else {
                    let entry = (mid.qualified(*fid), actuals);
                    // Only if this call has not been processed yet, queue it for future processing.
                    if !self.done_spec_funs.contains(&entry) {
                        self.todo_spec_funs.push(entry);
                    }
                }
            }
        });
    }

    // Type Analysis
    // -------------

    fn add_type_root(&mut self, ty: &Type) {
        if let Some(inst) = &self.inst_opt {
            let ty = ty.instantiate(inst);
            self.add_type(&ty)
        } else {
            self.add_type(ty)
        }
    }

    fn add_type(&mut self, ty: &Type) {
        if !self.done_types.insert(ty.to_owned()) {
            return;
        }
        ty.visit(&mut |t| match t {
            Type::Vector(et) => {
                self.info.vec_inst.insert(et.as_ref().clone());
            }
            Type::Datatype(mid, sid, targs) => {
                self.add_struct(self.env.get_module(*mid).into_struct(*sid), targs)
            }
            Type::TypeParameter(idx) => {
                self.info.type_params.insert(*idx);
            }
            _ => {}
        });
    }

    fn add_struct(&mut self, struct_: StructEnv<'_>, targs: &[Type]) {
        if struct_.is_intrinsic_of(INTRINSIC_TYPE_MAP) {
            self.info
                .table_inst
                .entry(struct_.get_qualified_id())
                .or_default()
                .insert((targs[0].clone(), targs[1].clone()));
        } else if struct_.is_native_or_intrinsic() && !targs.is_empty() {
            self.info
                .native_inst
                .entry(struct_.module_env.get_id())
                .or_default()
                .insert(targs.to_owned());
        } else {
            self.info
                .structs
                .entry(struct_.get_qualified_id())
                .or_default()
                .insert(targs.to_owned());
            for field in struct_.get_fields() {
                self.add_type(&field.get_type().instantiate(targs));
            }
        }
    }

    // Utility functions
    // -----------------

    fn add_types_in_borrow_edge(&mut self, edge: &BorrowEdge) {
        match edge {
            BorrowEdge::Direct | BorrowEdge::Index(_) => (),
            BorrowEdge::Field(qid, _) => {
                self.add_type_root(&qid.to_type());
            }
            BorrowEdge::Hyper(edges) => {
                for item in edges {
                    self.add_types_in_borrow_edge(item);
                }
            }
        }
    }
}
