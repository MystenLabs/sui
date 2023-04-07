// Copyright (c) The Diem Core Contributors
// Copyright (c) The Move Contributors
// SPDX-License-Identifier: Apache-2.0

// Analysis pass which analyzes how to injects global invariants into the bytecode.

use std::{
    collections::{BTreeMap, BTreeSet},
    fmt,
};

use move_binary_format::file_format::CodeOffset;
use move_model::{
    ast::ConditionKind,
    model::{FunId, FunctionEnv, GlobalEnv, GlobalId, QualifiedId, QualifiedInstId, StructId},
    ty::{Type, TypeDisplayContext, TypeInstantiationDerivation, TypeUnificationAdapter, Variance},
};

use crate::{
    function_target::{FunctionData, FunctionTarget},
    function_target_pipeline::{
        FunctionTargetProcessor, FunctionTargetsHolder, FunctionVariant, VerificationFlavor,
    },
    stackless_bytecode::{BorrowNode, Bytecode, Operation, PropKind},
    usage_analysis,
    verification_analysis::{is_invariant_suspendable, InvariantAnalysisData},
};

/// A named struct for holding the information on how an invariant is relevant to a bytecode.
#[derive(Default, Clone)]
pub struct PerBytecodeRelevance {
    /// for each `inst_fun` (instantiation of function type parameters) in the key set, the
    /// associated value is a set of `inst_inv` (instantiation of invariant type parameters) that
    /// are applicable to the concrete function instance `F<inst_fun>`.
    pub insts: BTreeMap<Vec<Type>, BTreeSet<Vec<Type>>>,
}

impl PerBytecodeRelevance {
    fn merge(&mut self, other: PerBytecodeRelevance) {
        for (fun_inst, inv_insts) in other.insts {
            self.insts.entry(fun_inst).or_default().extend(inv_insts);
        }
    }
}

/// A named struct for holding the information on how invariants are relevant to a function.
#[derive(Clone)]
pub struct PerFunctionRelevance {
    /// Invariants that needs to be assumed at function entrypoint
    /// - Key: global invariants that needs to be assumed before the first instruction,
    /// - Value: the instantiation information per each related invariant.
    pub entrypoint_assumptions: BTreeMap<GlobalId, PerBytecodeRelevance>,

    /// For each bytecode at given code offset, the associated value is a map of
    /// - Key: global invariants that needs to be asserted after the bytecode instruction and
    /// - Value: the instantiation information per each related invariant.
    pub per_bytecode_assertions: BTreeMap<CodeOffset, BTreeMap<GlobalId, PerBytecodeRelevance>>,

    /// Invariants that needs to be asserted at function exitpoint
    /// - Key: global invariants that needs to be assumed before the first instruction,
    /// - Value: the instantiation information per each related invariant.
    pub exitpoint_assertions: BTreeMap<GlobalId, PerBytecodeRelevance>,

    /// Number of ghost type parameters introduced in order to instantiate all asserted invariants
    pub ghost_type_param_count: usize,
}

/// Get verification information for this function.
pub fn get_info<'env>(target: &FunctionTarget<'env>) -> &'env PerFunctionRelevance {
    target
        .get_annotations()
        .get::<PerFunctionRelevance>()
        .expect("Global invariant analysis not performed")
}

// The function target processor
pub struct GlobalInvariantAnalysisProcessor {}

impl GlobalInvariantAnalysisProcessor {
    pub fn new() -> Box<Self> {
        Box::new(Self {})
    }
}

impl FunctionTargetProcessor for GlobalInvariantAnalysisProcessor {
    fn process(
        &self,
        targets: &mut FunctionTargetsHolder,
        fun_env: &FunctionEnv,
        mut data: FunctionData,
        _scc_opt: Option<&[FunctionEnv]>,
    ) -> FunctionData {
        if fun_env.is_native() || fun_env.is_intrinsic() {
            // Nothing to do
            return data;
        }
        if !data.variant.is_verified() {
            // Only need to instrument if this is a verification variant
            return data;
        }

        // Analyze invariants
        let target = FunctionTarget::new(fun_env, &data);
        let analysis_result = PerFunctionRelevance::analyze(&target, targets);
        // TODO(mengxu): re-verify that recursive functions do not impact how  global invariant
        // analysis are performed.
        data.annotations.set(analysis_result, true);

        // This is an analysis pass, nothing gets changed
        data
    }

    fn name(&self) -> String {
        "global_invariant_analysis".to_string()
    }

    fn dump_result(
        &self,
        f: &mut fmt::Formatter<'_>,
        env: &GlobalEnv,
        targets: &FunctionTargetsHolder,
    ) -> fmt::Result {
        // utils
        let type_display_ctxt = TypeDisplayContext::WithEnv {
            env,
            type_param_names: None,
        };

        let display_type_slice = |tys: &[Type]| -> String {
            let content = tys
                .iter()
                .map(|t| t.display(&type_display_ctxt).to_string())
                .collect::<Vec<_>>()
                .join(", ");
            format!("<{}>", content)
        };

        let make_indent = |indent: usize| "  ".repeat(indent);

        let display_inv_relevance = |f: &mut fmt::Formatter,
                                     invs: &BTreeMap<GlobalId, PerBytecodeRelevance>,
                                     header: &str,
                                     assert_or_assume: &str|
         -> fmt::Result {
            let mut indent = 1;

            // oneliner for empty invs
            if invs.is_empty() {
                return writeln!(f, "{}{} {{}}", make_indent(indent), header);
            }

            writeln!(f, "{}{} {{", make_indent(indent), header)?;
            indent += 1;

            for (inv_id, inv_rel) in invs {
                writeln!(
                    f,
                    "{}{} {} = [",
                    make_indent(indent),
                    assert_or_assume,
                    inv_id
                )?;
                indent += 1;

                for (rel_inst, inv_insts) in &inv_rel.insts {
                    writeln!(
                        f,
                        "{}{} -> [",
                        make_indent(indent),
                        display_type_slice(rel_inst)
                    )?;
                    indent += 1;

                    for inv_inst in inv_insts {
                        writeln!(f, "{}{}", make_indent(indent), display_type_slice(inv_inst))?;
                    }

                    indent -= 1;
                    writeln!(f, "{}]", make_indent(indent))?;
                }

                indent -= 1;
                writeln!(f, "{}]", make_indent(indent))?;
            }

            indent -= 1;
            writeln!(f, "{}}}", make_indent(indent))
        };

        writeln!(
            f,
            "\n********* Result of global invariant instrumentation *********\n"
        )?;
        for (fun_id, fun_variant) in targets.get_funs_and_variants() {
            if !matches!(
                fun_variant,
                FunctionVariant::Verification(VerificationFlavor::Regular)
            ) {
                // the analysis results are available in the regular verification variant
                continue;
            }

            let fenv = env.get_function(fun_id);
            let target = targets.get_target(&fenv, &fun_variant);
            let result = target
                .get_annotations()
                .get::<PerFunctionRelevance>()
                .expect("Analysis not performed");

            writeln!(f, "{}: [", fenv.get_full_name_str())?;

            // display entrypoint assumptions
            display_inv_relevance(f, &result.entrypoint_assumptions, "entrypoint", "assume")?;

            // display per-bytecode assertions
            for (code_offset, code_invs) in &result.per_bytecode_assertions {
                let bc = target.data.code.get(*code_offset as usize).unwrap();
                let header = format!("{}: {}", code_offset, bc.display(&target, &BTreeMap::new()));
                display_inv_relevance(f, code_invs, &header, "assert")?;
            }

            // display exitpoint assertions
            display_inv_relevance(f, &result.exitpoint_assertions, "exitpoint", "assert")?;

            writeln!(f, "]")?;
        }

        writeln!(f, "\n********* Global invariants by ID *********\n")?;
        let mut all_invs = BTreeSet::new();
        for menv in env.get_modules() {
            all_invs.extend(env.get_global_invariants_by_module(menv.get_id()));
        }
        for inv_id in all_invs {
            let inv = env.get_global_invariant(inv_id).unwrap();
            let inv_src = env.get_source(&inv.loc).unwrap_or("<unknown invariant>");
            writeln!(f, "{} => {}", inv_id, inv_src)?;
        }
        writeln!(f)
    }
}

/// This impl block is about the analysis pass
impl PerFunctionRelevance {
    /// Collect and build the relevance analysis information for this function target.
    fn analyze(target: &FunctionTarget, targets: &FunctionTargetsHolder) -> Self {
        use BorrowNode::*;
        use Bytecode::*;
        use Operation::*;

        // collect information
        let fid = target.func_env.get_qualified_id();
        let env = target.global_env();
        let inv_analysis = env
            .get_extension::<InvariantAnalysisData>()
            .expect("Verification analysis not performed");

        let check_suspendable_inv_on_return =
            inv_analysis.fun_set_with_inv_check_on_exit.contains(&fid);
        let inv_applicability = inv_analysis
            .fun_to_inv_map
            .get(&fid)
            .expect("Invariant applicability not available");
        let fun_type_params_arity = target.get_type_parameter_count();

        let inv_ro = &inv_applicability.accessed;
        let (inv_rw_return, inv_rw_normal): (BTreeSet<_>, BTreeSet<_>) =
            if check_suspendable_inv_on_return {
                inv_applicability
                    .direct_modified
                    .iter()
                    .cloned()
                    .partition(|inv_id| is_invariant_suspendable(env, *inv_id))
            } else {
                (BTreeSet::new(), inv_applicability.direct_modified.clone())
            };

        // collect invariant applicability and instantiation information per bytecode, i.e.,
        // - which invariants should be instrumented after each instruction and
        // - per each invariant applicable, how to instantiate them.
        let mut entrypoint_assumptions = BTreeMap::new();
        let mut per_bytecode_assertions = BTreeMap::new();
        let mut exitpoint_assertions = BTreeMap::new();
        let mut ghost_type_param_count = 0;

        for (code_offset, bc) in target.data.code.iter().enumerate() {
            let code_offset = code_offset as CodeOffset;

            // collect memory accessed/modified in operations
            let (mem_ro, mem_rw) = match bc {
                Call(_, _, oper, _, _) => match oper {
                    Function(mid, fid, inst) => {
                        let callee_fid = mid.qualified(*fid);
                        get_callee_memory_usage_for_invariant_instrumentation(
                            env, targets, callee_fid, inst,
                        )
                    }
                    OpaqueCallBegin(mid, fid, inst) => {
                        let callee_fid = mid.qualified(*fid);
                        let (mem_ro, _) = get_callee_memory_usage_for_invariant_instrumentation(
                            env, targets, callee_fid, inst,
                        );
                        (mem_ro, BTreeSet::new())
                    }
                    OpaqueCallEnd(mid, fid, inst) => {
                        let callee_fid = mid.qualified(*fid);
                        let (_, mem_rw) = get_callee_memory_usage_for_invariant_instrumentation(
                            env, targets, callee_fid, inst,
                        );
                        (BTreeSet::new(), mem_rw)
                    }

                    MoveTo(mid, sid, inst) | MoveFrom(mid, sid, inst) => {
                        let mem = mid.qualified_inst(*sid, inst.to_owned());
                        (BTreeSet::new(), std::iter::once(mem).collect())
                    }
                    WriteBack(GlobalRoot(mem), _) => {
                        (BTreeSet::new(), std::iter::once(mem.clone()).collect())
                    }

                    Exists(mid, sid, inst) | GetGlobal(mid, sid, inst) => {
                        let mem = mid.qualified_inst(*sid, inst.to_owned());
                        (std::iter::once(mem).collect(), BTreeSet::new())
                    }

                    // shortcut other operations
                    _ => continue,
                },

                Prop(_, PropKind::Assert, exp) | Prop(_, PropKind::Assume, exp) => (
                    exp.used_memory(env)
                        .into_iter()
                        .map(|(usage, _)| usage)
                        .collect(),
                    BTreeSet::new(),
                ),

                // shortcut other bytecodes
                _ => continue,
            };

            // collect instantiation information (step 1)
            // - entrypoint assumptions arised from memories that are read-only from the bytecode
            let relevance_ro = Self::calculate_invariant_relevance(
                env,
                mem_ro.iter(),
                inv_ro,
                fun_type_params_arity,
                &mut ghost_type_param_count,
                /* ignore_uninstantiated_invariant */ true,
            );

            // collect instantiation information (step 2)
            // - invariants that need to be assumed and asserted for read-write operations
            let relevance_rw_normal = Self::calculate_invariant_relevance(
                env,
                mem_rw.iter(),
                &inv_rw_normal,
                fun_type_params_arity,
                &mut ghost_type_param_count,
                /* ignore_uninstantiated_invariant */ false,
            );
            let relevance_rw_return = Self::calculate_invariant_relevance(
                env,
                mem_rw.iter(),
                &inv_rw_return,
                fun_type_params_arity,
                &mut ghost_type_param_count,
                /* ignore_uninstantiated_invariant */ false,
            );

            // entrypoint assumptions are about both the ro invariants and rw invariants, and
            // regardless of whether they are checked in-place or deferred to the exit point.
            for (inv_id, inv_rel) in relevance_ro
                .iter()
                .chain(relevance_rw_normal.iter())
                .chain(relevance_rw_return.iter())
            {
                let inv = env.get_global_invariant(*inv_id).unwrap();
                if matches!(inv.kind, ConditionKind::GlobalInvariantUpdate(..)) {
                    // update invariants should not be assumed at function entrypoint
                    continue;
                }
                entrypoint_assumptions
                    .entry(*inv_id)
                    .or_insert_with(PerBytecodeRelevance::default)
                    .merge(inv_rel.clone());
            }

            // normal rw invariants are asserted in-place, right after the bytecode
            per_bytecode_assertions.insert(code_offset, relevance_rw_normal);

            // exitpoint assertions are only about the rw invariants deferred to the exit point.
            for (inv_id, inv_rel) in relevance_rw_return {
                exitpoint_assertions
                    .entry(inv_id)
                    .or_insert_with(PerBytecodeRelevance::default)
                    .merge(inv_rel);
            }
        }

        // wrap and return the analysis result
        Self {
            entrypoint_assumptions,
            per_bytecode_assertions,
            exitpoint_assertions,
            ghost_type_param_count,
        }
    }

    /// Given a set of memories, calculate the global invariants that are related to this memory
    /// set and for each related global invariant, derive how to instantiate the invariant to make
    /// it relevant.
    fn calculate_invariant_relevance<'a>(
        env: &GlobalEnv,
        mem_related: impl Iterator<Item = &'a QualifiedInstId<StructId>>,
        inv_related: &BTreeSet<GlobalId>,
        fun_type_params_arity: usize,
        fun_type_params_ghost_count: &mut usize,
        ignore_uninstantiated_invariant: bool,
    ) -> BTreeMap<GlobalId, PerBytecodeRelevance> {
        let mut result = BTreeMap::new();

        for rel_mem in mem_related {
            let rel_ty = rel_mem.to_type();
            for inv_id in inv_related {
                let inv = env.get_global_invariant(*inv_id).unwrap();
                let inv_type_params = match &inv.kind {
                    ConditionKind::GlobalInvariant(params) => params,
                    ConditionKind::GlobalInvariantUpdate(params) => params,
                    _ => unreachable!(
                        "A global invariant must have a condition kind of either \
                            `GlobalInvariant` or `GlobalInvariantUpdate`"
                    ),
                };
                let inv_type_params_arity = inv_type_params.len();

                for inv_mem in &inv.mem_usage {
                    let inv_ty = inv_mem.to_type();

                    // make sure these two types unify before trying to instantiate them
                    let adapter = TypeUnificationAdapter::new_pair(&rel_ty, &inv_ty, true, true);
                    if adapter.unify(Variance::Allow, false).is_none() {
                        continue;
                    }

                    // instantiate the bytecode first
                    //
                    // NOTE: in fact, in this phase, we don't intend to instantiation the function
                    // nor do we want to collect information on how this function (or this bytecode)
                    // needs to be instantiated. All we care is how the invariant should be
                    // instantiated in order to be instrumented at this code point, with a generic
                    // function and generic code.
                    //
                    // But unfortunately, based on how the type unification logic is written now,
                    // this two-step instantiation is needed in order to find all possible
                    // instantiations of the invariant. I won't deny that there might be a way to
                    // collect invariant instantiation combinations without instantiating the
                    // function type parameters, but I haven't iron out one so far.
                    let rel_insts = TypeInstantiationDerivation::progressive_instantiation(
                        std::iter::once(&rel_ty),
                        std::iter::once(&inv_ty),
                        true,
                        true,
                        true,
                        false,
                        fun_type_params_arity,
                        true,
                        false,
                    );

                    // for each instantiation of the bytecode, instantiate the invariants
                    for rel_inst in rel_insts {
                        let inst_rel_ty = rel_ty.instantiate(&rel_inst);
                        let inv_insts = TypeInstantiationDerivation::progressive_instantiation(
                            std::iter::once(&inst_rel_ty),
                            std::iter::once(&inv_ty),
                            false,
                            true,
                            false,
                            true,
                            inv_type_params_arity,
                            false,
                            true,
                        );

                        let mut wellformed_inv_inst = vec![];
                        for inv_inst in inv_insts {
                            if inv_inst.iter().any(|t| matches!(t, Type::Error)) {
                                if ignore_uninstantiated_invariant {
                                    continue;
                                }
                                let adapted_inv_inst = inv_inst
                                    .into_iter()
                                    .map(|t| {
                                        if matches!(t, Type::Error) {
                                            let ghost_idx = fun_type_params_arity
                                                + *fun_type_params_ghost_count;
                                            *fun_type_params_ghost_count += 1;
                                            Type::TypeParameter(ghost_idx as u16)
                                        } else {
                                            t
                                        }
                                    })
                                    .collect();
                                wellformed_inv_inst.push(adapted_inv_inst);
                            } else {
                                wellformed_inv_inst.push(inv_inst);
                            }
                        }

                        // record the relevance information
                        result
                            .entry(*inv_id)
                            .or_insert_with(PerBytecodeRelevance::default)
                            .insts
                            .entry(rel_inst)
                            .or_insert_with(BTreeSet::new)
                            .extend(wellformed_inv_inst);
                    }
                }
            }
        }

        result
    }
}

fn get_callee_memory_usage_for_invariant_instrumentation(
    env: &GlobalEnv,
    targets: &FunctionTargetsHolder,
    callee_fid: QualifiedId<FunId>,
    callee_inst: &[Type],
) -> (
    BTreeSet<QualifiedInstId<StructId>>, // memory constitute to entry-point assumptions
    BTreeSet<QualifiedInstId<StructId>>, // memory constitute to in-line or exit-point assertions
) {
    let inv_analysis = env
        .get_extension::<InvariantAnalysisData>()
        .expect("Verification analysis not performed");

    let callee_env = env.get_function(callee_fid);
    let callee_target = targets.get_target(&callee_env, &FunctionVariant::Baseline);
    let callee_usage = usage_analysis::get_memory_usage(&callee_target);

    // NOTE: it is important to include *ALL* memories accessed/modified by the callee
    // instead of just the direct ones. Reasons include:
    // - if a function `F` delegates suspendable invariant checking to its caller,
    //   all the functions that `F` calls will not check suspendable invariants anymore.
    // - if a function `F` is inlined, then all its callee might be inlined as well and
    //   it is important to assume the invariants for them.
    let all_accessed = callee_usage.accessed.get_all_inst(callee_inst);
    if inv_analysis.fun_set_with_no_inv_check.contains(&callee_fid) {
        let mem_rw = callee_usage.modified.get_all_inst(callee_inst);
        let mem_ro = all_accessed.difference(&mem_rw).cloned().collect();
        (mem_ro, mem_rw)
    } else {
        (all_accessed, BTreeSet::new())
    }
}
