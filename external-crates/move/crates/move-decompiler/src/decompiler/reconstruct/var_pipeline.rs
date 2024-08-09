// Copyright (c) Verichains, 2023

use std::cell::RefCell;
use std::collections::{HashMap, HashSet};
use std::fmt::Debug;
use std::rc::Rc;

use crate::decompiler::cfg::metadata::WithMetadata;

use super::super::cfg::datastructs::*;
use super::super::cfg::*;

pub trait BranchMergeableVar {
    fn merge_branch(&self, other: &Self, base: &Option<&Self>) -> Option<Self>
    where
        Self: Sized;
}

pub trait TimeDeltableVar {
    type VarDelta;
    fn delta(&self, base: &Option<&Self>) -> Option<Self::VarDelta>
    where
        Self: Sized;
}

#[derive(Clone, Debug)]
pub struct VarPipelineState<Var>
where
    Var: Clone + Debug + Default + Sized,
{
    flow_id: usize,
    flow_id_pool: Rc<RefCell<usize>>,
    #[allow(dead_code)]
    time_container_id: usize,
    time_container_id_pool: Rc<RefCell<usize>>,
    var: HashMap<usize, Var>,
}

pub type VarPipelineStateRef<Var> = Box<VarPipelineState<Var>>;

impl<Var> VarPipelineState<Var>
where
    Var: Clone + Debug + Default + Sized,
{
    pub fn new() -> Self {
        Self {
            time_container_id: 0,
            time_container_id_pool: Rc::new(RefCell::new(0)),
            flow_id: 0,
            flow_id_pool: Rc::new(RefCell::new(0)),
            var: HashMap::new(),
        }
    }

    pub fn boxed(self: Self) -> VarPipelineStateRef<Var> {
        Box::new(self)
    }

    pub fn copy_as_ref(&self) -> VarPipelineStateRef<Var> {
        Box::new(self.clone())
    }

    pub fn new_initial(&self) -> VarPipelineStateRef<Var> {
        *self.time_container_id_pool.borrow_mut() += 1;
        *self.flow_id_pool.borrow_mut() += 1;
        Box::new(Self {
            flow_id: *self.flow_id_pool.borrow(),
            flow_id_pool: self.flow_id_pool.clone(),
            time_container_id: *self.time_container_id_pool.borrow(),
            time_container_id_pool: self.time_container_id_pool.clone(),
            var: HashMap::new(),
        })
    }

    pub fn copy_with_new_time(&self) -> VarPipelineStateRef<Var> {
        *self.time_container_id_pool.borrow_mut() += 1;
        Box::new(Self {
            flow_id: self.flow_id,
            flow_id_pool: self.flow_id_pool.clone(),
            time_container_id: *self.time_container_id_pool.borrow(),
            time_container_id_pool: self.time_container_id_pool.clone(),
            var: self.var.clone(),
        })
    }

    #[allow(dead_code)]
    pub fn time_id(&self) -> usize {
        self.time_container_id
    }

    pub fn get(&self, var: &usize) -> Option<&Var> {
        self.var.get(var)
    }

    pub fn get_or_default(&mut self, var: &usize) -> &mut Var {
        self.var.entry(*var).or_insert_with(|| Var::default())
    }

    pub fn snapshot(&self) -> HashMap<usize, Var> {
        self.var.clone()
    }
}

impl<Var> VarPipelineState<Var>
where
    Var: Clone + Debug + Default + Sized + BranchMergeableVar,
{
    pub fn merge_branches<F: Fn(&Var) -> Var>(&self, children: Vec<&Self>, to_parent: F) -> Self {
        let mut vars: HashMap<usize, Var> = HashMap::new();
        let mut merged_flow_ids: HashSet<(usize, usize)> = HashSet::new();
        for child in children {
            for (k, v) in child.var.iter() {
                let v_in_parent = to_parent(v);
                let var_flow_id = (*k, child.flow_id);
                if let Some(cv) = vars.get(k) {
                    let base = if merged_flow_ids.contains(&var_flow_id) {
                        self.var.get(k)
                    } else {
                        None
                    };
                    if let Some(new_var) = cv.merge_branch(&v_in_parent, &base) {
                        vars.insert(*k, new_var);
                    }
                } else {
                    vars.insert(*k, v_in_parent);
                }
                merged_flow_ids.insert(var_flow_id);
            }
        }

        Self {
            var: vars,
            ..self.clone()
        }
    }
}

impl<Var> VarPipelineState<Var>
where
    Var: Clone + Debug + Default + Sized + TimeDeltableVar,
{
    pub fn delta(&self, base: &Self) -> HashMap<usize, Var::VarDelta> {
        let mut vars = HashMap::new();
        for (k, v) in self.var.iter() {
            if let Some(new_var) = v.delta(&base.var.get(k)) {
                vars.insert(*k, new_var);
            }
        }

        for (k, v) in base.var.iter() {
            if self.var.contains_key(k) {
                continue;
            }
            if let Some(new_var) = Var::default().delta(&Some(v)) {
                vars.insert(*k, new_var);
            }
        }

        vars
    }
}

pub trait VarPipelineRunner<Config, Var>
where
    Var: Clone + Debug + Default + Sized,
{
    fn run_unit(
        &self,
        config: &Config,
        state: &VarPipelineStateRef<Var>,
        unit_block: &mut WithMetadata<CodeUnitBlock<usize, StacklessBlockContent>>,
    ) -> Result<VarPipelineStateRef<Var>, anyhow::Error>;

    fn run_hyperblock(
        &self,
        config: &Config,
        state: &VarPipelineStateRef<Var>,
        hyper_block: &mut WithMetadata<HyperBlock<usize, StacklessBlockContent>>,
    ) -> Result<VarPipelineStateRef<Var>, anyhow::Error>;

    fn run_basicblock(
        &self,
        config: &Config,
        state: &VarPipelineStateRef<Var>,
        basic_block: &mut WithMetadata<BasicBlock<usize, StacklessBlockContent>>,
    ) -> Result<VarPipelineStateRef<Var>, anyhow::Error>;
}
