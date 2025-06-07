// Copyright (c) Verichains, 2023

use std::collections::HashSet;

use crate::decompiler::evaluator::stackless::StacklessEvaluationRunResult;

use super::{
    cfg::{datastructs::Terminator, metadata::WithMetadata},
    Naming,
};
use anyhow::Ok;
use move_model::model::FunctionEnv;
use move_stackless_bytecode::{function_target::FunctionTarget, stackless_bytecode::Operation};

use self::{
    ast::expr::{DecompiledExpr, DecompiledExprRef},
    stackless_var_usage::{VarUsage, VarUsageSnapshot},
    var_pipeline::{VarPipelineState, VarPipelineStateRef},
};

use super::{
    cfg::{
        datastructs::{BasicBlock, CodeUnitBlock, HyperBlock},
        StacklessBlockContent,
    },
    evaluator::stackless::{ReturnValueHint, StacklessEvaluationContext},
};

pub use self::ast::optimizers::OptimizerSettings;

mod ast;
pub mod code_unit;
mod stackless_var_alias;
mod stackless_var_usage;
mod var_pipeline;
use ast::*;
use code_unit::*;

pub struct SourceGen<'a> {
    var_usage: VarPipelineStateRef<VarUsage>,
    func_env: &'a FunctionEnv<'a>,
    func_target: &'a FunctionTarget<'a>,
    naming: Naming<'a>,
    body: &'a mut WithMetadata<CodeUnitBlock<usize, StacklessBlockContent>>,
}

#[derive(Clone, Debug)]
struct DatatypeureCtx {
    is_tail: bool,
    is_top_most_block: bool,
    outer_propagating_vars: HashSet<usize>,
    block_final_var_usage: VarUsageSnapshot<VarUsage>,
}

impl DatatypeureCtx {
    fn new() -> Self {
        Self {
            is_tail: true,
            is_top_most_block: true,
            outer_propagating_vars: HashSet::new(),
            block_final_var_usage: VarUsageSnapshot::default(),
        }
    }

    fn enter_block(&mut self) {
        self.is_top_most_block = false;
        self.outer_propagating_vars.clear();
    }

    fn apply_is_tail(&mut self, is_tail: bool) {
        self.is_tail = self.is_tail && is_tail;
    }

    fn need_propagate(&mut self, var: usize) {
        self.outer_propagating_vars.insert(var);
    }
}

impl<'a> SourceGen<'a> {
    pub fn new(
        body: &'a mut WithMetadata<CodeUnitBlock<usize, StacklessBlockContent>>,
        func_env: &'a FunctionEnv<'a>,
        func_target: &'a FunctionTarget<'a>,
        naming: &'a Naming,
    ) -> Self {
        Self {
            body,
            func_env,
            func_target,
            naming: naming.with_arg_count(func_env.get_parameter_count()),
            var_usage: VarPipelineState::new().boxed(),
        }
    }

    pub(crate) fn generate(
        &mut self,
        optimizer_settings: &OptimizerSettings,
    ) -> Result<SourceCodeUnit, anyhow::Error> {
        let mut alias_solver = stackless_var_alias::VarAliasChecker::new();

        let mut evaluation_ctx = StacklessEvaluationContext::new(self.func_env);
        let mut predefined_variables = Vec::new();
        for i in self.func_target.get_parameters() {
            predefined_variables.push(i);
        }
        let mut nonalias_variables = Vec::new();
        for i in 0..self.func_target.get_local_count() {
            let typ = self.func_target.get_local_type(i);
            if typ.is_mutable_reference() {
                nonalias_variables.push(i);
            }
        }
        let aliasset =
            alias_solver.calculate(self.body, &predefined_variables, &nonalias_variables)?;
        for set in aliasset.sets.iter() {
            let mut set: Vec<_> = set.iter().collect();
            set.sort();
            let mut iter = set.iter();
            let first = *iter.next().unwrap();
            for &v in iter {
                evaluation_ctx.add_pre_defined_alias(v, first);
            }
        }

        for i in self.func_target.get_parameters() {
            evaluation_ctx.flush_local_value(i, Some(true));
        }

        let variable_usage_runner = stackless_var_usage::StacklessVarUsagePipeline::new(self.func_env.module_env.env);
        self.var_usage = variable_usage_runner.run(self.body)?;

        let mut cfg_context = DatatypeureCtx::new();

        let ast = self.visit_codeunit(&mut evaluation_ctx, &mut cfg_context, self.body)?;

        if evaluation_ctx.pop_branch_condition().is_some() {
            return Err(anyhow::anyhow!("final branch condition stack not empty"));
        }

        let (ast, referenced_vairables) = ast::optimizers::run(
            &ast,
            self.func_target,
            &self.naming,
            optimizer_settings,
            evaluation_ctx.aliases(),
        )?;

        let final_naming = self.naming.with_referenced_variables(&referenced_vairables);

        Ok(ast.to_source(&final_naming, true)?)
    }

    // this function check with the assumption that the variable's value has no dependency
    fn can_ignore_variable_assignment(
        &self,
        v: usize,
        s_ctx: &DatatypeureCtx,
        var_usage: &VarUsageSnapshot<VarUsage>,
    ) -> bool {
        if let Some(v) = self.var_usage.get(&v) {
            if v.should_keep_as_variable {
                return false;
            }
        }

        let final_var_usage = &s_ctx.block_final_var_usage.backward_run_pre.1.get(&v);
        let final_var_usage = if let Some(v) = final_var_usage {
            (*v).clone()
        } else {
            VarUsage::default()
        };

        let forward_var_usage = &var_usage.backward_run_pre.1.get(&v);
        let forward_var_usage = if let Some(v) = forward_var_usage {
            (*v).clone()
        } else {
            VarUsage::default()
        };

        if final_var_usage.write_cnt != forward_var_usage.write_cnt {
            return false;
        }

        if s_ctx.outer_propagating_vars.contains(&v) {
            forward_var_usage.max_read_cnt_max_from_cfg - final_var_usage.max_read_cnt_max_from_cfg
                <= 1
        } else {
            forward_var_usage.max_read_cnt_max_from_cfg <= 1
        }
    }

    /// the variable should be declared if the referenced values may be changed before its last read
    /// we're here if the variable already checked for the following:
    ///  - has trivial value
    ///  - has not already defined
    ///  - ignore check above accepted
    fn check_need_declare(
        &self,
        _code_unit: &mut DecompiledCodeUnitRef, // in case we need to add comments
        dst: usize,
        _is_variable_copy_assignment: bool,
        result: &super::evaluator::stackless::expr_node::Expr,
        evaluation_ctx: &StacklessEvaluationContext<'_>,
        s_ctx: &DatatypeureCtx,
        node_var_usage: &VarUsageSnapshot<VarUsage>,
    ) -> bool {
        let deps: HashSet<_> = result
            .collect_variables_with_count(false, true)
            .variables
            .keys()
            .cloned()
            .collect();
        let deps = &deps;

        let mut queue: HashSet<usize> = deps.clone();
        let mut visited: HashSet<usize> = deps.clone();

        while !queue.is_empty() {
            let u = *queue.iter().next().unwrap();
            queue.remove(&u);

            if !evaluation_ctx.defined(u) {
                // how can this be possible?
                unreachable!();
            }

            let u_value = evaluation_ctx.get_var(u);
            if u_value.is_flushed() {
                continue;
            }

            let u_deps = u_value.collect_variables_with_count(false, true).variables;
            for (v, _) in u_deps {
                if !visited.contains(&v) {
                    queue.insert(v);
                    visited.insert(v);
                }
            }
        }

        // trivial case: no dependency
        if deps.is_empty() {
            return false;
        }

        // trivial case: depend on itself
        if deps.contains(&dst) {
            return true;
        }

        // trivial case: no dependency has future write
        {
            let forward_state = &node_var_usage.backward_run_pre.1;
            if deps.iter().all(|x| {
                if let Some(s) = forward_state.get(x) {
                    s.write_cnt == 0
                } else {
                    true
                }
            }) {
                return false;
            };
        };

        // trivial case: this variable only live in this block, and no dependency has write until the end of this block
        {
            let forward_state = &node_var_usage.backward_run_pre.1;
            let final_var_usage = &s_ctx.block_final_var_usage.backward_run_pre.1;
            let no_usage_outside = s_ctx.outer_propagating_vars.contains(&dst)
                || if let Some(usage) = final_var_usage.get(&dst) {
                    usage.write_cnt == 0 && usage.read_cnt == 0
                } else {
                    true
                };

            if no_usage_outside {
                let dst_var = forward_state.get(&dst);
                if dst_var.is_none() {
                    // no usage from this point on? it's a dead variable.
                    // let's keep it for understanding
                    return false;
                }

                if deps.iter().all(|d: &usize| {
                    if let Some(dusg) = forward_state.get(d) {
                        !dusg.vars_read_before_last_write.contains(&dst)
                    } else {
                        true
                    }
                }) {
                    return false;
                }
            };
        };

        //TODO: implement a good algorithm to check this

        // for now, we cant make sure, let's assume it's needed
        true
    }

    fn check_need_declare_due_to_mut_ref(
        &self,
        op: &Operation,
        srcs: &[usize],
        node_var_usage: &VarUsageSnapshot<VarUsage>,
    ) -> bool {
        if let Operation::Function(mid, fid, _types) = op{
            let module = self.func_env.module_env.env.get_module(*mid);
            let func = module.get_function(*fid);
            for (idx, param) in func.get_parameters().iter().enumerate() {
                let ty = &param.1;
                if ty.is_mutable_reference() {
                    let src = srcs[idx];
                    let forward_state = &node_var_usage.backward_run_pre.1;
                    if let Some(usage) = forward_state.get(&src) {
                        if usage.read_cnt > 0 {
                            return true;
                        }
                    }
                }
            }
        }

        return false;
    }

    fn visit_codeunit(
        &self,
        evaluation_ctx: &mut StacklessEvaluationContext,
        s_ctx: &DatatypeureCtx,
        current: &WithMetadata<CodeUnitBlock<usize, StacklessBlockContent>>,
    ) -> Result<DecompiledCodeUnitRef, anyhow::Error> {
        let mut codeunit = DecompiledCodeUnit::new();
        let mut iter = current.inner().blocks.iter().peekable();
        let mut s_ctx = s_ctx.clone();
        let future_need_vars = s_ctx.outer_propagating_vars.clone();

        s_ctx.block_final_var_usage = current
            .meta()
            .get::<VarUsageSnapshot<VarUsage>>()
            .unwrap()
            .clone();

        while let Some(block) = iter.next() {
            let mut next_inner_s_ctx = s_ctx.clone();
            next_inner_s_ctx.apply_is_tail(iter.peek().is_none());
            let block = self.visit_hyperblock(evaluation_ctx, &next_inner_s_ctx, &block)?;
            codeunit.extends(block)?;
        }

        if !current.is_terminated_in_loop() {
            if !future_need_vars.is_empty() {
                let mut variables = future_need_vars.iter().cloned().collect::<Vec<_>>();
                variables.sort();
                let variables_expr = if future_need_vars.len() > 1 {
                    DecompiledExpr::Tuple(
                        variables
                            .iter()
                            .map(|x| {
                                if evaluation_ctx.defined(*x) {
                                    DecompiledExpr::EvaluationExpr(
                                        evaluation_ctx.get_var(*x).copy(),
                                    )
                                    .boxed()
                                } else {
                                    DecompiledExpr::Undefined.boxed()
                                }
                            })
                            .collect::<Vec<_>>(),
                    )
                    .boxed()
                } else {
                    let x = *variables.iter().next().unwrap();
                    if evaluation_ctx.defined(x) {
                        DecompiledExprRef::new(DecompiledExpr::EvaluationExpr(
                            evaluation_ctx.get_var(x).copy(),
                        ))
                    } else {
                        DecompiledExprRef::new(DecompiledExpr::Undefined)
                    }
                };
                codeunit.exit(variables, variables_expr, false)?;
            }
        }

        Ok(codeunit)
    }

    fn visit_hyperblock(
        &self,
        evaluation_ctx: &mut StacklessEvaluationContext<'_>,
        s_ctx: &DatatypeureCtx,
        block: &WithMetadata<HyperBlock<usize, StacklessBlockContent>>,
    ) -> Result<DecompiledCodeUnitRef, anyhow::Error> {
        let mut codeunit = DecompiledCodeUnit::new();

        match block.inner() {
            HyperBlock::ConnectedBlocks(blocks) => {
                let mut iter = blocks.iter().peekable();
                while let Some(block) = iter.next() {
                    let mut inner_s_ctx = s_ctx.clone();
                    inner_s_ctx.apply_is_tail(iter.peek().is_none());

                    let block = self.visit_basicblock(evaluation_ctx, &mut inner_s_ctx, block)?;
                    codeunit.extends(block)?;
                }
            }

            HyperBlock::IfElseBlocks { if_unit, else_unit } => {
                let cond = evaluation_ctx.pop_branch_condition();
                if cond.is_none() {
                    return Err(anyhow::anyhow!("fail to obtain branch condition"));
                }
                let cond = cond.unwrap();

                let mut t_vars =
                    find_need_propagate_inner_defining_variables(if_unit, evaluation_ctx);
                let mut f_vars =
                    find_need_propagate_inner_defining_variables(else_unit, evaluation_ctx);

                let if_terminated = if_unit.is_terminated_in_loop();
                let else_terminated = else_unit.is_terminated_in_loop();

                if if_terminated != else_terminated {
                    if if_unit.is_terminated_in_loop() {
                        t_vars = f_vars.clone();
                    }
                    if else_unit.is_terminated_in_loop() {
                        f_vars = t_vars.clone();
                    }
                }

                let if_vars: HashSet<_> = if let (Some(t_vars), Some(f_vars)) = (&t_vars, &f_vars) {
                    t_vars.intersection(&f_vars).cloned().collect()
                } else {
                    HashSet::new()
                };
                let mut need_declares = HashSet::new();
                for vs in [&t_vars, &f_vars] {
                    if let Some(vs) = vs {
                        vs.difference(&if_vars).for_each(|&v| {
                            need_declares.insert(v);
                        });
                    }
                }
                let mut need_declares = need_declares.iter().cloned().collect::<Vec<_>>();
                need_declares.sort();

                for v in &need_declares {
                    codeunit.add(DecompiledCodeItem::PreDeclareStatement { variable: *v });
                    evaluation_ctx.flush_local_value(*v, None);
                }

                let mut t_ctx = evaluation_ctx.clone();
                let mut f_ctx = evaluation_ctx.clone();

                let mut t_s_ctx = s_ctx.clone();
                t_s_ctx.enter_block();
                let mut f_s_ctx = s_ctx.clone();
                f_s_ctx.enter_block();

                t_vars.as_ref().map(|tv| {
                    tv.intersection(&if_vars)
                        .for_each(|&v| t_s_ctx.need_propagate(v))
                });
                f_vars.as_ref().map(|tv| {
                    tv.intersection(&if_vars)
                        .for_each(|&v| f_s_ctx.need_propagate(v))
                });

                let tu = self.visit_codeunit(&mut t_ctx, &t_s_ctx, if_unit.as_ref())?;
                let fu = self.visit_codeunit(&mut f_ctx, &f_s_ctx, else_unit.as_ref())?;

                let meta = block.meta();
                let var_usage = meta.get::<VarUsageSnapshot<VarUsage>>().unwrap();

                let mut branches_to_merge = Vec::new();

                if !if_unit.is_terminated() {
                    branches_to_merge.push(&t_ctx);
                }

                if !else_unit.is_terminated() {
                    branches_to_merge.push(&f_ctx);
                }

                if branches_to_merge.is_empty() {
                } else if branches_to_merge.len() == 1 {
                    *evaluation_ctx = branches_to_merge[0].clone();
                } else {
                    for v in evaluation_ctx.merge_branches(&branches_to_merge, true) {
                        let has_future_usage =
                            if let Some(usage) = var_usage.backward_run_pre.1.get(&v) {
                                usage.read_cnt + usage.write_cnt > 0
                            } else {
                                false
                            };
                        if !has_future_usage {
                            continue;
                        }
                        evaluation_ctx.flush_local_value(v, Some(true));
                    }
                }

                let mut result_variables = if_vars.iter().cloned().collect::<Vec<_>>();
                result_variables.sort();

                codeunit.add(DecompiledCodeItem::IfElseStatement {
                    cond: DecompiledExpr::EvaluationExpr(cond).boxed(),
                    if_unit: tu,
                    else_unit: fu,
                    result_variables,
                    use_as_result: ResultUsageType::None,
                });
            }

            HyperBlock::WhileBlocks {
                inner,
                outer,
                unconditional,
                ..
            } => {
                let unconditional = *unconditional;
                let cond = if unconditional {
                    None
                } else {
                    let cond = evaluation_ctx.pop_branch_condition();
                    if cond.is_none() {
                        return Err(anyhow::anyhow!("fail to obtain branch condition"));
                    }

                    Some(DecompiledExpr::EvaluationExpr(cond.unwrap()).boxed())
                };

                let inner_var_meta = inner.as_ref().meta();
                let inner_var_usage = inner_var_meta.get::<VarUsageSnapshot<VarUsage>>().unwrap();

                let local_variables = inner_var_usage
                    .forward_run_post
                    .iter()
                    .filter(|(v, _)| !evaluation_ctx.defined_or_pending(**v))
                    .map(|(v, _)| *v)
                    .collect::<Vec<_>>();

                let need_pre_declaring_variables = local_variables
                    .iter()
                    .filter(|v| {
                        if let Some(usage) = inner_var_usage.backward_run_pre.1.get(v) {
                            usage.read_cnt + usage.write_cnt > 0
                        } else {
                            false
                        }
                    })
                    .cloned()
                    .collect::<Vec<_>>();

                for v in &need_pre_declaring_variables {
                    codeunit.add(DecompiledCodeItem::PreDeclareStatement { variable: *v });
                    evaluation_ctx.flush_local_value(*v, None);
                }

                let mut i_ctx = evaluation_ctx.clone();
                i_ctx.enter_loop();

                let mut i_s_ctx = s_ctx.clone();

                i_s_ctx.enter_block();
                i_s_ctx.apply_is_tail(false);

                let inner = self.visit_codeunit(&mut i_ctx, &mut i_s_ctx, inner.as_ref())?;
                for v in evaluation_ctx.merge_branches(&vec![&i_ctx], false) {
                    let has_future_usage =
                        if let Some(usage) = inner_var_usage.backward_run_pre.1.get(&v) {
                            usage.read_cnt + usage.write_cnt > 0
                        } else {
                            false
                        };

                    if !has_future_usage {
                        continue;
                    }

                    if evaluation_ctx.defined(v) {
                        evaluation_ctx.flush_local_value(v, Some(true));
                    } else {
                        return Err(anyhow::anyhow!(
                            "while loop should not have leaked variables"
                        ));
                    }
                }

                codeunit.add(DecompiledCodeItem::WhileStatement { cond, body: inner });

                let outer = self.visit_codeunit(evaluation_ctx, s_ctx, outer.as_ref())?;

                codeunit.extends(outer)?;
            }
        }

        Ok(codeunit)
    }

    fn visit_basicblock(
        &self,
        evaluation_ctx: &mut StacklessEvaluationContext<'_>,
        s_ctx: &DatatypeureCtx,
        block: &WithMetadata<BasicBlock<usize, StacklessBlockContent>>,
    ) -> Result<DecompiledCodeUnitRef, anyhow::Error> {
        let mut codeunit = DecompiledCodeUnit::new();
        let mut iter = block
            .inner()
            .content
            .code
            .iter()
            .filter(|x| !x.removed)
            .peekable();

        while let Some(bytecode) = iter.next() {
            let node_var_usage = bytecode
                .meta()
                .get::<VarUsageSnapshot<VarUsage>>()
                .unwrap()
                .clone();

            use move_stackless_bytecode::stackless_bytecode::Bytecode::*;

            let dst_tmps = match &bytecode.bytecode {
                Load(_, dst, _) | Assign(_, dst, _, _) => vec![*dst],

                Call(_, dsts, _, _, _) => dsts.iter().cloned().collect::<Vec<_>>(),

                _ => vec![],
            };

            let dst_types: Vec<_> = dst_tmps
                .iter()
                .map(|x| {
                    Some(ReturnValueHint {
                        ty: self.func_target.get_local_type(*x).clone(),
                    })
                })
                .collect();

            let StacklessEvaluationRunResult {
                results: result,
                new_variables,
                flushed_variables: pre_flushed,
                cannot_keep_as_expr,
            } = evaluation_ctx.run(&bytecode.bytecode, &dst_types)?;

            if result.should_ignore() {
                continue;
            }

            match &bytecode.bytecode {
                Assign(_, dst, _, _) => {
                    let dst = *dst;
                    let is_new = new_variables.contains(&dst);
                    let dst_value = evaluation_ctx.get_var(dst);
                    if dst_value.is_non_trivial()
                        || pre_flushed.contains(&dst)
                        || cannot_keep_as_expr
                        || !self.can_ignore_variable_assignment(dst, s_ctx, &node_var_usage)
                        || self.check_need_declare(
                            &mut codeunit,
                            dst,
                            true,
                            &result,
                            &evaluation_ctx,
                            s_ctx,
                            &node_var_usage,
                        )
                    {
                        codeunit.add(DecompiledCodeItem::AssignStatement {
                            variable: dst,
                            value: DecompiledExpr::EvaluationExpr(result).boxed(),
                            is_decl: is_new,
                        });

                        evaluation_ctx.flush_local_value(dst, Some(is_new));
                    } else {
                        let assignment_id = evaluation_ctx.flush_pending_local_value(
                            dst,
                            Some(is_new),
                            result.copy(),
                        );

                        codeunit.add(DecompiledCodeItem::PossibleAssignStatement {
                            assignment_id,
                            variable: dst,
                            value: DecompiledExpr::EvaluationExpr(result.copy()).boxed(),
                            is_decl: is_new,
                        });
                    }
                }

                Call(_, dsts, op, srcs, _) => {
                    use super::evaluator::stackless::expr_node::ExprNodeOperation as E;
                    if let E::DatatypeUnpack(name, fields, val, _types) =
                        &result.value().borrow().operation
                    {
                        // special case: unpack to no variable
                        if dsts.is_empty() {
                            codeunit.add(DecompiledCodeItem::AssignStructureStatement {
                                structure_visible_name: name.clone(),
                                variables: Vec::new(),
                                value: DecompiledExpr::EvaluationExpr(
                                    val.borrow().operation.to_expr(),
                                )
                                .boxed(),
                            });
                        } else {
                            if fields.len() != dsts.len() {
                                return Err(anyhow::anyhow!("struct unpack field count mismatch"));
                            }

                            codeunit.add(DecompiledCodeItem::AssignStructureStatement {
                                structure_visible_name: name.clone(),
                                variables: fields
                                    .iter()
                                    .zip(dsts.iter())
                                    .map(|(field, dst)| (field.clone(), *dst))
                                    .collect::<Vec<_>>(),
                                value: DecompiledExpr::EvaluationExpr(
                                    val.borrow().operation.to_expr(),
                                )
                                .boxed(),
                            });

                            dsts.iter().for_each(|&dst| {
                                evaluation_ctx.flush_local_value(dst, Some(true));
                            });
                        }
                    } else if dsts.len() > 1 {
                        let is_new = !new_variables.is_empty();

                        codeunit.add(DecompiledCodeItem::AssignTupleStatement {
                            variables: dsts.clone(),
                            value: DecompiledExpr::EvaluationExpr(result.copy()).boxed(),
                            is_decl: is_new,
                        });

                        dsts.iter().for_each(|&dst| {
                            evaluation_ctx.flush_local_value(dst, Some(is_new));
                        });
                    } else if dsts.len() == 1 {
                        let dst = dsts[0];
                        let dst_value = evaluation_ctx.get_var(dst);
                        let is_new = new_variables.contains(&dst);

                        if dst_value.is_non_trivial()
                            || pre_flushed.contains(&dst)
                            || cannot_keep_as_expr
                            || !self.can_ignore_variable_assignment(dst, s_ctx, &node_var_usage)
                            || self.check_need_declare(
                                &mut codeunit,
                                dst,
                                false,
                                &result,
                                &evaluation_ctx,
                                s_ctx,
                                &node_var_usage,
                            )
                            || self.check_need_declare_due_to_mut_ref(
                                op,
                                srcs,
                                &node_var_usage,
                            )
                        {
                            codeunit.add(DecompiledCodeItem::AssignStatement {
                                variable: dst,
                                value: DecompiledExpr::EvaluationExpr(result.copy()).boxed(),
                                is_decl: is_new,
                            });

                            evaluation_ctx.flush_local_value(dst, Some(is_new));
                        } else {
                            let assignment_id = evaluation_ctx.flush_pending_local_value(
                                dst,
                                Some(is_new),
                                result.copy(),
                            );

                            codeunit.add(DecompiledCodeItem::PossibleAssignStatement {
                                assignment_id,
                                variable: dst,
                                value: DecompiledExpr::EvaluationExpr(result.copy()).boxed(),
                                is_decl: is_new,
                            });
                        }
                    } else {
                        codeunit.add(DecompiledCodeItem::Statement {
                            expr: DecompiledExpr::EvaluationExpr(result.copy()).boxed(),
                        });
                    }
                }

                Ret(_, srcs) => codeunit.add(DecompiledCodeItem::ReturnStatement(
                    DecompiledExpr::Tuple(
                        srcs.iter()
                            .map(|x| {
                                DecompiledExpr::EvaluationExpr(evaluation_ctx.get_var(*x).copy())
                                    .boxed()
                            })
                            .collect::<Vec<_>>(),
                    )
                    .boxed(),
                )),

                Abort(_, src) => {
                    codeunit.add(DecompiledCodeItem::AbortStatement(
                        DecompiledExpr::EvaluationExpr(evaluation_ctx.get_var(*src).copy()).boxed(),
                    ));
                }

                Load(_, dst, val) => {
                    let dst = *dst;
                    let dst_value = evaluation_ctx.get_var(dst);

                    // we dont need cycle reference in this case, as val is a constant
                    if dst_value.is_non_trivial()
                        || pre_flushed.contains(&dst)
                        || !self.can_ignore_variable_assignment(dst, s_ctx, &node_var_usage)
                    {
                        let is_new = new_variables.contains(&dst);
                        codeunit.add(DecompiledCodeItem::AssignStatement {
                            variable: dst,
                            value: DecompiledExpr::EvaluationExpr(
                                crate::decompiler::evaluator::stackless::expr_node::ExprNodeOperation::Const(
                                    val.clone(),
                                )
                                .to_expr(),
                            )
                            .boxed(),
                            is_decl: is_new,
                        });
                        evaluation_ctx.flush_local_value(dst, Some(is_new));
                    }
                }

                Branch(_, _t, _f, src) => {
                    let src_value = evaluation_ctx.get_var(*src);
                    if src_value.is_non_trivial() {
                        unreachable!("branch on non trivial condition");
                    }
                    use super::cfg::datastructs::JumpType;
                    match bytecode.jump_type {
                        JumpType::While | JumpType::Unknown | JumpType::If => {}
                        JumpType::Continue | JumpType::Break => {
                            unreachable!(
                                    "continue and break jump opcode should not exists for conditional node"
                                );
                        }
                    }

                    evaluation_ctx.push_branch_condition(src_value.copy())?;
                }

                Jump(_, lbl) => {
                    // should have been removed
                    codeunit.add(DecompiledCodeItem::CommentStatement(format!(
                        "goto {}",
                        lbl.as_usize()
                    )));
                }

                Label(_, lbl) => {
                    // should have been removed
                    codeunit.add(DecompiledCodeItem::CommentStatement(format!(
                        "label {}",
                        lbl.as_usize()
                    )));
                }

                Nop(_) => {}

                VariantSwitch(_, _, _) => todo!(),
            }
        }

        let block = block.inner();
        match block.next {
            Terminator::Normal
            | Terminator::Ret
            | Terminator::Abort
            | Terminator::IfElse { .. }
            | Terminator::Branch { .. }
            | Terminator::While { .. } => {}

            Terminator::Break { .. } => {
                if !block.implicit_terminator {
                    codeunit.add(DecompiledCodeItem::BreakStatement);
                }
            }

            Terminator::Continue { .. } => {
                if !block.implicit_terminator {
                    codeunit.add(DecompiledCodeItem::ContinueStatement);
                }
            }
        };

        Ok(codeunit)
    }
}

fn find_need_propagate_inner_defining_variables(
    current: &WithMetadata<CodeUnitBlock<usize, StacklessBlockContent>>,
    evaluation_ctx: &StacklessEvaluationContext<'_>,
) -> Option<HashSet<usize>> {
    if current.is_terminated() {
        return None;
    }

    let meta = current.meta();
    let snapshot = meta.get::<VarUsageSnapshot<VarUsage>>().unwrap();
    let self_var_access = &snapshot.forward_run_post;
    let future_vars_usage = &snapshot.backward_run_pre.1;
    let mut r = HashSet::new();

    for (v, usage_delta) in self_var_access.iter() {
        if !evaluation_ctx.defined(*v)
            && usage_delta.write_cnt > 0
            && future_vars_usage
                .get(v)
                .map(|x| x.read_cnt + x.write_cnt > 0)
                .unwrap_or(false)
        {
            r.insert(*v);
        }
    }

    Some(r)
}
