// Copyright (c) Verichains
// Copyright © Aptos Foundation
// SPDX-License-Identifier: Apache-2.0

use std::{
    cell::{RefCell, RefMut},
    collections::{HashMap, HashSet},
};

use super::{
    super::cfg::{
        algo::blocks_stackless::AnnotatedBytecode,
        datastructs::{BasicBlock, CodeUnitBlock, HyperBlock},
        metadata::WithMetadata,
        StacklessBlockContent,
    },
    var_pipeline::{
        BranchMergeableVar, TimeDeltableVar, VarPipelineRunner, VarPipelineState,
        VarPipelineStateRef,
    },
};

pub trait VarUsageSnapshotWithDelta {
    type Delta;
}

#[derive(Clone, Debug)]
pub struct VarUsage {
    pub read_cnt: usize,
    pub write_cnt: usize,

    pub vars_written_before_last_read: HashSet<usize>,
    pub vars_read_before_last_write: HashSet<usize>,

    pub max_read_cnt_max_from_cfg: usize,
    pub should_keep_as_variable: bool,
}

impl Default for VarUsage {
    fn default() -> Self {
        Self {
            read_cnt: 0,
            write_cnt: 0,

            vars_written_before_last_read: Default::default(),
            vars_read_before_last_write: Default::default(),

            max_read_cnt_max_from_cfg: Default::default(),
            should_keep_as_variable: Default::default(),
        }
    }
}

#[derive(Clone, Debug, Default)]
pub struct VarUsageDelta {
    pub read_cnt: isize,
    pub write_cnt: isize,
}

impl VarUsageSnapshotWithDelta for VarUsage {
    type Delta = VarUsageDelta;
}

impl TimeDeltableVar for VarUsage {
    type VarDelta = VarUsageDelta;
    fn delta(&self, base: &Option<&Self>) -> Option<Self::VarDelta> {
        match base {
            Some(base) => Some(Self::VarDelta {
                read_cnt: self.read_cnt as isize - base.read_cnt as isize,
                write_cnt: self.write_cnt as isize - base.write_cnt as isize,
            }),

            None => Some(Self::VarDelta {
                read_cnt: self.read_cnt as isize,
                write_cnt: self.write_cnt as isize,
            }),
        }
    }
}

#[derive(Clone, Debug, Default)]
pub struct VarUsageSnapshot<T: Default + Clone + VarUsageSnapshotWithDelta> {
    // snapshots usage excluding current block
    pub backward_run_pre: (usize, HashMap<usize, T>),
    // snapshots usage excluding current block
    pub forward_run_pre: (usize, HashMap<usize, T>),

    pub forward_run_post: HashMap<usize, T::Delta>,
}

impl VarUsage {
    fn add_read(&mut self, written_before: &HashSet<usize>) {
        self.read_cnt += 1;
        self.max_read_cnt_max_from_cfg += 1;
        self.vars_written_before_last_read.extend(written_before);
    }

    fn add_write(&mut self, read_before: &HashSet<usize>) {
        self.write_cnt += 1;
        self.vars_read_before_last_write.extend(read_before);
    }
}

impl BranchMergeableVar for VarUsage {
    fn merge_branch(&self, other: &Self, base: &Option<&Self>) -> Option<Self> {
        let (base_read_cnt, base_write_cnt) = match base {
            Some(base) => (base.read_cnt, base.write_cnt),

            None => (0, 0),
        };

        Some(Self {
            read_cnt: self.read_cnt + other.read_cnt - base_read_cnt,
            write_cnt: self.write_cnt + other.write_cnt - base_write_cnt,

            vars_written_before_last_read: self
                .vars_written_before_last_read
                .union(&other.vars_written_before_last_read)
                .cloned()
                .collect(),

            vars_read_before_last_write: self
                .vars_read_before_last_write
                .union(&other.vars_read_before_last_write)
                .cloned()
                .collect(),

            should_keep_as_variable: self.should_keep_as_variable || other.should_keep_as_variable,
            max_read_cnt_max_from_cfg: self
                .max_read_cnt_max_from_cfg
                .max(other.max_read_cnt_max_from_cfg),
        })
    }
}

pub struct StacklessVarUsagePipeline<'s> {
    env: &'s move_model::model::GlobalEnv,
}

#[derive(Clone, Debug)]
struct UnitConfig {
    written_before: HashSet<usize>,
    read_before: HashSet<usize>,
}

impl UnitConfig {
    fn new() -> Self {
        Self {
            written_before: Default::default(),
            read_before: Default::default(),
        }
    }

    fn merge_with(&mut self, other: &UnitConfig) {
        self.written_before.extend(&other.written_before);
        self.read_before.extend(&other.read_before);
    }
}

struct ForwardVisitorConfig {
    unit_config: RefCell<UnitConfig>,
    t: RefCell<usize>,
}
struct BackwardVisitorConfig {
    unit_config: RefCell<UnitConfig>,
    t: RefCell<usize>,
}

impl<'s> StacklessVarUsagePipeline<'s> {
    pub fn new(env: &'s move_model::model::GlobalEnv) -> Self {
        Self {
            env,
        }
    }

    pub fn run(
        &self,
        unit: &mut WithMetadata<CodeUnitBlock<usize, StacklessBlockContent>>,
    ) -> Result<VarPipelineStateRef<VarUsage>, anyhow::Error> {
        self.run_unit(
            &BackwardVisitorConfig {
                unit_config: RefCell::new(UnitConfig::new()),
                t: RefCell::new(0),
            },
            &VarPipelineState::new().boxed(),
            unit,
        )?;

        self.run_unit(
            &ForwardVisitorConfig {
                unit_config: RefCell::new(UnitConfig::new()),
                t: RefCell::new(0),
            },
            &VarPipelineState::new().boxed(),
            unit,
        )
    }

    fn update_state(
        &self,
        inst: &AnnotatedBytecode,
        unit_config: &mut RefMut<UnitConfig>,
        state: &mut Box<VarPipelineState<VarUsage>>,
        running_forward: bool,
        _t: usize,
    ) {
        use move_stackless_bytecode::stackless_bytecode::Bytecode::*;
        match &inst.bytecode {
            Assign(_, dst, src, _) => {
                let svar = state.get_or_default(src);
                svar.add_read(&unit_config.written_before);
                unit_config.read_before.insert(*src);
                let dvar = state.get_or_default(dst);
                dvar.add_write(&unit_config.read_before);
                unit_config.written_before.insert(*dst);
            }
    
            Call(_, dsts, op, srcs, _) => {
                use move_stackless_bytecode::stackless_bytecode::Operation;
                if matches!(op, Operation::Destroy) {
                    for s in srcs {
                        let svar = state.get_or_default(s);
                        svar.should_keep_as_variable = true;
                    }
                    return;
                }
                for &src in srcs {
                    let svar = state.get_or_default(&src);
                    svar.add_read(&unit_config.written_before);
                }
                unit_config.read_before.extend(srcs);

                if let Operation::Function(mid, fid, _types) = op{
                    let module = self.env.get_module(*mid);
                    let func = module.get_function(*fid);

                    for (idx, param) in func.get_parameters().iter().enumerate() {
                        let ty = &param.1;
                        if ty.is_mutable_reference() {
                            let src = srcs[idx];
                            let svar = state.get_or_default(&src);
                            // we don't know if there would be a write to the reference,
                            // so we conservatively assume that there would be a write
                            svar.add_write(&unit_config.read_before);
                        }
                    }
                }
                
                for dst in dsts {
                    let dvar = state.get_or_default(dst);
                    dvar.add_write(&unit_config.read_before);
                }
    
                unit_config.written_before.extend(dsts);
                if running_forward {
                    if matches!(op, Operation::Pack(..) | Operation::Unpack(..)) {
                        for dst in dsts {
                            let dvar = state.get_or_default(dst);
                            dvar.should_keep_as_variable = true;
                        }
                    }
                    if matches!(op, Operation::BorrowLoc) {
                        for src in srcs {
                            let svar = state.get_or_default(src);
                            svar.should_keep_as_variable = true;
                        }
                    }
                }
            }
    
            Ret(_, srcs) => {
                for &src in srcs {
                    let svar = state.get_or_default(&src);
                    svar.add_read(&unit_config.written_before);
                }
                unit_config.read_before.extend(srcs);
            }
    
            Branch(_, _, _, src) | Abort(_, src) | VariantSwitch(_, src, _) => {
                let svar = state.get_or_default(src);
                svar.add_read(&unit_config.written_before);
                unit_config.read_before.insert(*src);
            }
    
            Load(_, dst, _) => {
                let dvar = state.get_or_default(dst);
                dvar.add_write(&unit_config.read_before);
                unit_config.written_before.insert(*dst);
            }
    
            Jump(..) | Label(..) | Nop(..) => {}
        }
    }
}

impl<'s> VarPipelineRunner<BackwardVisitorConfig, VarUsage> for StacklessVarUsagePipeline<'s> {
    fn run_unit(
        &self,
        config: &BackwardVisitorConfig,
        state: &VarPipelineStateRef<VarUsage>,
        unit_block: &mut WithMetadata<CodeUnitBlock<usize, StacklessBlockContent>>,
    ) -> Result<VarPipelineStateRef<VarUsage>, anyhow::Error> {
        let mut state = state.copy_with_new_time();

        unit_block
            .meta_mut()
            .get_or_default::<VarUsageSnapshot<VarUsage>>()
            .backward_run_pre = (*config.t.borrow(), state.snapshot());

        for block in &mut unit_block.inner_mut().blocks.iter_mut().rev() {
            state = self.run_hyperblock(config, &state, block)?;
        }

        Ok(state)
    }

    fn run_hyperblock(
        &self,
        config: &BackwardVisitorConfig,
        state: &VarPipelineStateRef<VarUsage>,
        hyper_block: &mut WithMetadata<HyperBlock<usize, StacklessBlockContent>>,
    ) -> Result<VarPipelineStateRef<VarUsage>, anyhow::Error> {
        let mut state = state.copy_as_ref();

        hyper_block
            .meta_mut()
            .get_or_default::<VarUsageSnapshot<VarUsage>>()
            .backward_run_pre = (*config.t.borrow(), state.snapshot());

        match hyper_block.inner_mut() {
            HyperBlock::ConnectedBlocks(blocks) => {
                for block in blocks.iter_mut().rev() {
                    state = self.run_basicblock(config, &state, block)?;
                }
            }

            HyperBlock::IfElseBlocks { if_unit, else_unit } => {
                let saved_unit_config = config.unit_config.borrow().clone();
                let state_t = self.run_unit(config, &state, if_unit.as_mut())?;
                let t_unit_config = config.unit_config.borrow().clone();
                *config.unit_config.borrow_mut() = saved_unit_config;
                let state_f = self.run_unit(config, &state, else_unit.as_mut())?;
                config.unit_config.borrow_mut().merge_with(&t_unit_config);
                state = state
                    .merge_branches(vec![&state_t, &state_f], |s| s.clone())
                    .boxed();
            }

            HyperBlock::WhileBlocks { inner, outer, .. } => {
                state = self.run_unit(config, &state, outer.as_mut())?;
                let o_unit_config = config.unit_config.borrow().clone();
                let state_i = self.run_unit(config, &state, inner.as_mut())?;
                config.unit_config.borrow_mut().merge_with(&o_unit_config);
                state = state.merge_branches(vec![&state_i], |s| s.clone()).boxed();
            }
        };
        Ok(state)
    }

    fn run_basicblock(
        &self,
        config: &BackwardVisitorConfig,
        state: &VarPipelineStateRef<VarUsage>,
        basic_block: &mut WithMetadata<BasicBlock<usize, StacklessBlockContent>>,
    ) -> Result<VarPipelineStateRef<VarUsage>, anyhow::Error> {
        let mut state = if basic_block.inner().next.is_terminated() {
            state.new_initial()
        } else {
            state.copy_as_ref()
        };

        basic_block
            .meta_mut()
            .get_or_default::<VarUsageSnapshot<VarUsage>>()
            .backward_run_pre = (*config.t.borrow(), state.snapshot());

        for inst in basic_block.inner_mut().content.code.iter_mut().rev() {
            *config.t.borrow_mut() += 1;
            inst.meta_mut()
                .get_or_default::<VarUsageSnapshot<VarUsage>>()
                .backward_run_pre = (*config.t.borrow(), state.snapshot());
            self.update_state(
                inst,
                &mut config.unit_config.borrow_mut(),
                &mut state,
                false,
                *config.t.borrow(),
            );
        }

        Ok(state)
    }
}

impl<'s> VarPipelineRunner<ForwardVisitorConfig, VarUsage> for StacklessVarUsagePipeline<'s> {
    fn run_unit(
        &self,
        config: &ForwardVisitorConfig,
        state: &VarPipelineStateRef<VarUsage>,
        unit_block: &mut WithMetadata<CodeUnitBlock<usize, StacklessBlockContent>>,
    ) -> Result<VarPipelineStateRef<VarUsage>, anyhow::Error> {
        let original_t = *config.t.borrow();

        let mut state = state.copy_with_new_time();
        let original_state = state.copy_as_ref();
        *config.t.borrow_mut() = 0;

        unit_block
            .meta_mut()
            .get_or_default::<VarUsageSnapshot<VarUsage>>()
            .forward_run_pre = (*config.t.borrow(), state.snapshot());

        for block in &mut unit_block.inner_mut().blocks {
            state = self.run_hyperblock(config, &state, block)?;
        }

        unit_block
            .meta_mut()
            .get_or_default::<VarUsageSnapshot<VarUsage>>()
            .forward_run_post = state.delta(original_state.as_ref());

        *config.t.borrow_mut() = original_t;

        Ok(state)
    }

    fn run_hyperblock(
        &self,
        config: &ForwardVisitorConfig,
        state: &VarPipelineStateRef<VarUsage>,
        hyper_block: &mut WithMetadata<HyperBlock<usize, StacklessBlockContent>>,
    ) -> Result<VarPipelineStateRef<VarUsage>, anyhow::Error> {
        let original_state = state.copy_as_ref();
        let mut state = state.copy_as_ref();
        hyper_block
            .meta_mut()
            .get_or_default::<VarUsageSnapshot<VarUsage>>()
            .forward_run_pre = (*config.t.borrow(), state.snapshot());
        match hyper_block.inner_mut() {
            HyperBlock::ConnectedBlocks(blocks) => {
                for block in blocks.iter_mut() {
                    state = self.run_basicblock(config, &state, block)?;
                }
            }

            HyperBlock::IfElseBlocks { if_unit, else_unit } => {
                let saved_unit_config = config.unit_config.borrow().clone();
                let state_t = self.run_unit(config, &state, if_unit.as_mut())?;
                let t_unit_config = config.unit_config.borrow().clone();
                *config.unit_config.borrow_mut() = saved_unit_config;
                let state_f = self.run_unit(config, &state, else_unit.as_mut())?;
                config.unit_config.borrow_mut().merge_with(&t_unit_config);
                state = state
                    .merge_branches(vec![&state_t, &state_f], |x| x.clone())
                    .boxed();
            }

            HyperBlock::WhileBlocks { inner, outer, .. } => {
                let saved_unit_config = config.unit_config.borrow().clone();
                let state_i = self.run_unit(config, &state, inner.as_mut())?;
                config
                    .unit_config
                    .borrow_mut()
                    .merge_with(&saved_unit_config);
                state = state.merge_branches(vec![&state_i], |s| s.clone()).boxed();
                state = self.run_unit(config, &state, outer.as_mut())?;
            }
        }

        hyper_block
            .meta_mut()
            .get_or_default::<VarUsageSnapshot<VarUsage>>()
            .forward_run_post = state.delta(original_state.as_ref());

        Ok(state)
    }

    fn run_basicblock(
        &self,
        config: &ForwardVisitorConfig,
        state: &VarPipelineStateRef<VarUsage>,
        basic_block: &mut WithMetadata<BasicBlock<usize, StacklessBlockContent>>,
    ) -> Result<VarPipelineStateRef<VarUsage>, anyhow::Error> {
        let original_state = state.copy_as_ref();
        let mut state: Box<VarPipelineState<VarUsage>> = state.copy_as_ref();

        basic_block
            .meta_mut()
            .get_or_default::<VarUsageSnapshot<VarUsage>>()
            .forward_run_pre = (*config.t.borrow(), state.snapshot());

        for inst in basic_block.inner_mut().content.code.iter_mut() {
            *config.t.borrow_mut() += 1;
            let prev_state = state.copy_as_ref();
            inst.meta_mut()
                .get_or_default::<VarUsageSnapshot<VarUsage>>()
                .forward_run_pre = (*config.t.borrow(), state.snapshot());
            self.update_state(
                inst,
                &mut config.unit_config.borrow_mut(),
                &mut state,
                true,
                *config.t.borrow(),
            );
            inst.meta_mut()
                .get_or_default::<VarUsageSnapshot<VarUsage>>()
                .forward_run_post = state.delta(prev_state.as_ref());
        }

        basic_block
            .meta_mut()
            .get_or_default::<VarUsageSnapshot<VarUsage>>()
            .forward_run_post = state.delta(original_state.as_ref());

        Ok(state)
    }
}