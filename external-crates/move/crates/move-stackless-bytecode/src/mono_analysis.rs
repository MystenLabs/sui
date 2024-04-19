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
    model::{DatatypeId, FunId, GlobalEnv, ModuleId, QualifiedId, StructEnv},
    ty::{Type, TypeDisplayContext},
    well_known::{TYPE_INFO_MOVE, TYPE_NAME_GET_MOVE, TYPE_NAME_MOVE},
};

use crate::{
    function_target::FunctionTarget,
    function_target_pipeline::{FunctionTargetProcessor, FunctionTargetsHolder, FunctionVariant},
    stackless_bytecode::{BorrowEdge, Bytecode, Operation},
};

/// The environment extension computed by this analysis.
#[derive(Clone, Default, Debug)]
pub struct MonoInfo {
    pub structs: BTreeMap<QualifiedId<DatatypeId>, BTreeSet<Vec<Type>>>,
    pub funs: BTreeMap<(QualifiedId<FunId>, FunctionVariant), BTreeSet<Vec<Type>>>,
    pub type_params: BTreeSet<u16>,
    pub vec_inst: BTreeSet<Type>,
    pub table_inst: BTreeMap<QualifiedId<DatatypeId>, BTreeSet<(Type, Type)>>,
    pub native_inst: BTreeMap<ModuleId, BTreeSet<Vec<Type>>>,
    pub all_types: BTreeSet<Type>,
}

/// Get the information computed by this analysis.
pub fn get_info(env: &GlobalEnv) -> Rc<MonoInfo> {
    env.get_extension::<MonoInfo>()
        .unwrap_or_else(|| Rc::new(MonoInfo::default()))
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
        writeln!(f, "\n\n==== mono-analysis result ====\n")?;
        let info = env
            .get_extension::<MonoInfo>()
            .expect("monomorphization analysis not run");
        let tctx = TypeDisplayContext::WithEnv {
            env,
            type_param_names: None,
        };
        let display_inst = |tys: &[Type]| {
            tys.iter()
                .map(|ty| ty.display(&tctx).to_string())
                .join(", ")
        };
        for (sid, insts) in &info.structs {
            let sname = env.get_struct(*sid).get_full_name_str();
            writeln!(f, "struct {} = {{", sname)?;
            for inst in insts {
                writeln!(f, "  <{}>", display_inst(inst))?;
            }
            writeln!(f, "}}")?;
        }
        for ((fid, variant), insts) in &info.funs {
            let fname = env.get_function(*fid).get_full_name_str();
            writeln!(f, "fun {} [{}] = {{", fname, variant)?;
            for inst in insts {
                writeln!(f, "  <{}>", display_inst(inst))?;
            }
            writeln!(f, "}}")?;
        }
        for (module, insts) in &info.native_inst {
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

        Ok(())
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
            done_types: BTreeSet::new(),
            inst_opt: None,
        };
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
                    if !variant.is_verified() {
                        continue;
                    }
                    self.analyze_fun(target.clone());
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
                .entry((fun, variant.clone()))
                .or_default()
                .insert(inst.clone());
            self.done_funs.insert((fun, variant, inst));
        }
    }

    fn analyze_fun(&mut self, target: FunctionTarget<'_>) {
        // Analyze function locals and return value types.
        for idx in 0..target.get_local_count() {
            self.add_type_root(target.get_local_type(idx));
        }
        for ty in target.get_return_types().iter() {
            self.add_type_root(ty);
        }
        // Analyze code.
        if !target.func_env.is_native() {
            for bc in target.get_bytecode() {
                self.analyze_bytecode(&target, bc);
            }
        }
    }

    fn analyze_bytecode(&mut self, _target: &FunctionTarget<'_>, bc: &Bytecode) {
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

                if callee_env.is_native() && !actuals.is_empty() {
                    // Mark the associated module to be instantiated with the given actuals.
                    // This will instantiate all functions in the module with matching number
                    // of type parameters.
                    self.info
                        .native_inst
                        .entry(callee_env.module_env.get_id())
                        .or_default()
                        .insert(actuals);
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
            _ => {}
        }
    }

    fn instantiate_vec(&self, targs: &[Type]) -> Vec<Type> {
        if let Some(inst) = &self.inst_opt {
            Type::instantiate_slice(targs, inst)
        } else {
            targs.to_owned()
        }
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
        if struct_.is_native() && !targs.is_empty() {
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
