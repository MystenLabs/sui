// Copyright (c) Verichains, 2023

use std::{
    cell::RefCell,
    collections::{HashMap, HashSet, VecDeque},
    rc::Rc,
};

use crate::decompiler::cfg::{
    datastructs::{BasicBlock, CodeUnitBlock, HyperBlock},
    metadata::WithMetadata,
    StacklessBlockContent,
};

// a is aliased to b := reading a is equivalent to reading b

/// The alias graph is a DAG, an edge directed from a to b is an alias relation "a is aliased to b"
/// Each node has at most one outgoing edge, and can have multiple incoming edges.
#[derive(Clone, Debug, Default)]
struct AliasMapN1 {
    fr_to: HashMap<usize, usize>,
    to_fr: HashMap<usize, HashSet<usize>>,
}

impl AliasMapN1 {
    fn new() -> Self {
        Self {
            fr_to: HashMap::new(),
            to_fr: HashMap::new(),
        }
    }

    fn internal_remove_to_fr(&mut self, fr: usize, to: usize) {
        if self.to_fr.get_mut(&to).map(|s| {
            s.remove(&fr);
            s.is_empty()
        }) == Some(true)
        {
            self.to_fr.remove(&to);
        }
    }

    fn has_alias_chain(&self, fr: usize, to: usize) -> bool {
        let mut visited = HashSet::new();
        let mut fr = fr;
        while fr != to {
            debug_assert!(visited.insert(fr), "alias cycle detected");
            fr = match self.fr_to.get(&fr) {
                Some(to) => *to,
                None => return false,
            }
        }
        fr == to
    }

    fn insert(&mut self, fr: usize, to: usize) {
        debug_assert!(!self.has_alias_chain(to, fr));
        if let Some(current_to) = self.fr_to.get(&fr) {
            if current_to == &to {
                return;
            }
            self.internal_remove_to_fr(fr, *current_to);
        }

        self.fr_to.insert(fr, to);
        self.to_fr.entry(to).or_default().insert(fr);
    }

    fn vars_aliased_to(&self, target: usize) -> HashSet<usize> {
        let mut visited = HashSet::new();
        let mut queue = VecDeque::new();
        queue.push_back(target);
        visited.insert(target);
        while let Some(fr) = queue.pop_front() {
            self.to_fr.get(&fr).map(|s| {
                for to in s.iter() {
                    if visited.insert(*to) {
                        queue.push_back(*to);
                    }
                }
            });
        }
        visited.remove(&target);
        visited
    }

    fn remove_var(&mut self, var: usize) {
        if let Some(to) = self.fr_to.get(&var) {
            self.internal_remove_to_fr(var, *to);
        }
        self.fr_to.remove(&var);
        self.to_fr.get(&var).map(|s| {
            for fr in s.iter() {
                self.fr_to.remove(fr);
            }
        });
        self.to_fr.remove(&var);
    }
}

#[derive(Clone, Debug, Default)]
pub struct VarAliasState {
    time_provider: Rc<RefCell<usize>>,
    var_time: HashMap<usize, usize>,
    non_alias: HashSet<usize>,
    invalidated_vars: HashSet<usize>,
    defined_vars: HashSet<usize>,
    map: AliasMapN1,
}

#[derive(Clone, Debug, Default)]
pub struct VarAliasStateContainer {
    state: VarAliasState,
    break_snapshots: Rc<RefCell<Vec<Box<VarAliasState>>>>,
}

impl VarAliasState {
    pub fn new() -> Self {
        Self {
            time_provider: Rc::new(RefCell::new(0)),
            var_time: HashMap::new(),
            defined_vars: HashSet::new(),
            invalidated_vars: HashSet::new(),
            non_alias: HashSet::new(),
            map: AliasMapN1::new(),
        }
    }

    fn fork(&self) -> VarAliasState {
        VarAliasState {
            time_provider: self.time_provider.clone(),
            var_time: self.var_time.clone(),
            defined_vars: self.defined_vars.clone(),
            invalidated_vars: self.invalidated_vars.clone(),
            non_alias: self.non_alias.clone(),
            map: self.map.clone(),
        }
    }

    fn next_time(&self) -> usize {
        let mut time = self.time_provider.borrow_mut();
        *time += 1;
        *time
    }

    fn update_from_branches(&mut self, states: &[&VarAliasState]) {
        self.invalidated_vars.clear();
        self.non_alias.clear();
        self.var_time.clear();
        let mut conflict_var_time = HashSet::new();

        let mut relations_unique_check = HashSet::new();
        let mut relations = Vec::new();
        for state in states.iter() {
            self.defined_vars.extend(state.defined_vars.iter());
            self.invalidated_vars.extend(state.invalidated_vars.iter());
            self.non_alias.extend(state.non_alias.iter());
            for (fr, to) in state.map.fr_to.iter() {
                if relations_unique_check.insert((*fr, *to)) {
                    relations.push((*fr, *to));
                }
            }
            for (v, t) in state.var_time.iter() {
                if conflict_var_time.contains(v) {
                    continue;
                }
                if let Some(current_t) = self.var_time.get(v) {
                    if current_t != t {
                        conflict_var_time.insert(*v);
                        continue;
                    }
                }
                self.var_time.insert(*v, *t);
            }
        }
        for v in conflict_var_time.iter() {
            self.invalidated_vars.insert(*v);
            self.var_time.insert(*v, self.next_time());
        }
        self.map = AliasMapN1::new();
        for (fr, to) in relations.into_iter() {
            let mut has_conflict = false;
            for state in states.iter() {
                if state.map.has_alias_chain(to, fr) {
                    has_conflict = true;
                    break;
                }
            }
            if has_conflict {
                continue;
            }

            if self.map.fr_to.contains_key(&fr) {
                continue;
            }
            if self.non_alias.contains(&fr) {
                continue;
            }
            if self.map.has_alias_chain(to, fr) {
                continue;
            }
            self.map.insert(fr, to);
        }
    }

    fn var_write(&mut self, var: &usize, src_var: Option<&usize>) {
        let time = if let Some(src_var) = src_var {
            self.var_time.get(src_var).copied().unwrap_or_else(|| {
                if cfg!(debug_assertions) {
                    panic!("src var {:?} not defined", src_var);
                }
                self.next_time()
            })
        } else {
            self.next_time()
        };
        self.var_time.insert(*var, time);
        self.defined_vars.insert(*var);
        for v in self.map.vars_aliased_to(*var) {
            self.invalidated_vars.insert(v);
        }
        if self.map.to_fr.contains_key(var) || self.map.fr_to.contains_key(var) {
            self.invalidated_vars.insert(*var);
        }
    }

    fn var_read(&mut self, var: &usize) {
        if self.invalidated_vars.contains(var) {
            // when an read happens to an invalidated var, it's confirmed to be non-aliased
            self.non_alias.insert(*var);
            self.map.remove_var(*var);
            self.invalidated_vars.remove(var);
        }
    }

    fn add_alias(&mut self, fr: usize, to: usize) {
        if fr == to {
            // write to self is a no-op
            return;
        }
        self.var_read(&to);
        self.var_write(&fr, Some(&to));

        // there maybe a case that a temporary variable is created for computation and then assigned back
        // let v1 = f();
        // let v0 = v1;
        // the code use both v0 and v1 afterwards

        if
        // both fr and to are non-alias
        (self.non_alias.contains(&fr) && self.non_alias.contains(&to))
        // or adding this will make a cycle to -> ... -> fr -> to
        || self.map.has_alias_chain(to, fr)
        {
            return;
        }

        self.map.insert(fr, to);
    }

    fn get_alias_sets(&self) -> Vec<HashSet<usize>> {
        let mut sets = vec![];
        let mut visited = HashSet::new();
        for start in self.map.fr_to.keys() {
            if self.non_alias.contains(start) {
                continue;
            }
            if !visited.insert(start) {
                continue;
            }
            let mut queue = VecDeque::new();
            queue.push_back(*start);
            let mut set = HashSet::new();
            set.insert(*start);
            while let Some(fr) = queue.pop_front() {
                self.map.to_fr.get(&fr).map(|s| {
                    for to in s.iter() {
                        if !visited.insert(to) || self.non_alias.contains(to) {
                            continue;
                        }
                        queue.push_back(*to);
                        set.insert(*to);
                    }
                });
                self.map.fr_to.get(&fr).map(|to| {
                    if !visited.insert(to) || self.non_alias.contains(to) {
                        return;
                    }
                    queue.push_back(*to);
                    set.insert(*to);
                });
            }
            if set.len() > 1 {
                sets.push(set);
            }
        }
        sets
    }
}

impl VarAliasStateContainer {
    fn new() -> Self {
        Self {
            state: VarAliasState::new(),
            break_snapshots: Rc::new(RefCell::new(vec![])),
        }
    }
    fn fork(&self) -> Self {
        Self {
            state: self.state.fork(),
            break_snapshots: self.break_snapshots.clone(),
        }
    }
    fn fork_for_loop(&self) -> Self {
        let mut s = self.fork();
        s.break_snapshots = Rc::new(RefCell::new(vec![]));
        s
    }

    fn terminate(&mut self) {
        // nothing to do
    }

    fn add_break_snapshot(&self) {
        self.break_snapshots
            .borrow_mut()
            .push(Box::new(self.state.clone()));
    }

    fn update_from_branches(&mut self, states: &[&VarAliasStateContainer]) {
        self.state
            .update_from_branches(&states.iter().map(|s| &s.state).collect::<Vec<_>>());
    }

    fn update_from_break_snapshots(&mut self) {
        self.state.update_from_branches(
            &self
                .break_snapshots
                .borrow()
                .iter()
                .rev()
                .map(|s| s.as_ref())
                .collect::<Vec<_>>(),
        );
    }

    fn assign(&mut self, dst: &usize, src: &usize) {
        self.state.add_alias(*dst, *src);
    }

    fn var_read(&mut self, vars: &Vec<usize>) {
        for v in vars.iter() {
            self.state.var_read(v);
        }
    }

    fn var_write(&mut self, vars: &Vec<usize>) {
        for v in vars.iter() {
            self.state.var_write(v, None);
        }
        if vars.len() > 1 {
            // disable aliasing for tuple assignment variables
            self.state.non_alias.extend(vars.iter());
            for v in vars.iter() {
                self.state.map.remove_var(*v);
            }
        }
    }

    fn get_alias_sets(&self) -> Vec<HashSet<usize>> {
        return self.state.get_alias_sets();
    }
}

pub struct VarAliasChecker {}

pub struct AliasSet {
    pub(crate) sets: Vec<HashSet<usize>>,
}

impl VarAliasChecker {
    pub fn new() -> Self {
        Self {}
    }

    pub fn calculate(
        &mut self,
        unit: &WithMetadata<CodeUnitBlock<usize, StacklessBlockContent>>,
        predefined_vars: &Vec<usize>,
        non_alias_vars: &Vec<usize>,
    ) -> Result<AliasSet, anyhow::Error> {
        let mut state = VarAliasStateContainer::new();
        for v in predefined_vars.iter() {
            state.var_write(&vec![*v]);
        }
        for v in non_alias_vars.iter() {
            state.state.non_alias.insert(*v);
        }
        let state = self.run_unit(&state, unit)?;
        let sets = state.get_alias_sets();
        Ok(AliasSet { sets })
    }

    fn run_unit(
        &self,
        state: &VarAliasStateContainer,
        unit_block: &WithMetadata<CodeUnitBlock<usize, StacklessBlockContent>>,
    ) -> Result<VarAliasStateContainer, anyhow::Error> {
        let mut state = state.clone();
        for block in unit_block.inner().blocks.iter() {
            state = self.run_hyperblock(&state, block)?;
        }
        Ok(state)
    }

    fn run_hyperblock(
        &self,
        state: &VarAliasStateContainer,
        hyper_block: &WithMetadata<HyperBlock<usize, StacklessBlockContent>>,
    ) -> Result<VarAliasStateContainer, anyhow::Error> {
        let mut state = state.clone();
        match &hyper_block.inner() {
            HyperBlock::ConnectedBlocks(blocks) => {
                for block in blocks.iter() {
                    state = self.run_basicblock(&state, block)?;
                }
            }
            HyperBlock::IfElseBlocks { if_unit, else_unit } => {
                state = state.fork();
                let state_t = self.run_unit(&state, if_unit)?;
                let state_f = self.run_unit(&state, else_unit)?;

                state.update_from_branches(&[&state_t, &state_f]);
            }
            HyperBlock::WhileBlocks {
                inner,
                outer,
                unconditional,
                ..
            } => {
                let state_inner = state.fork_for_loop();
                if !*unconditional {
                    state_inner.add_break_snapshot();
                }
                let mut state_inner = self.run_unit(&state_inner, inner)?;
                if !*unconditional || !inner.is_terminated_in_loop() {
                    state_inner.add_break_snapshot();
                }
                state_inner.update_from_break_snapshots();
                state.state = state_inner.state;

                state = self.run_unit(&state, outer)?;
            }
        }

        Ok(state)
    }

    fn run_basicblock(
        &self,
        state: &VarAliasStateContainer,
        basic_block: &WithMetadata<BasicBlock<usize, StacklessBlockContent>>,
    ) -> Result<VarAliasStateContainer, anyhow::Error> {
        let mut state = state.clone();
        for inst in basic_block.inner().content.code.iter() {
            let inst = inst.inner();
            use move_stackless_bytecode::stackless_bytecode::Bytecode as StacklessBytecode;
            match &inst.bytecode {
                StacklessBytecode::Assign(_, dst, src, _) => {
                    state.assign(dst, src);
                }
                StacklessBytecode::Call(_, dsts, op, srcs, _) => {
                    use move_stackless_bytecode::stackless_bytecode::Operation;
                    if matches!(op, Operation::Destroy) {
                        continue;
                    }
                    state.var_read(srcs);
                    state.var_write(dsts);
                }
                StacklessBytecode::Ret(_, srcs) => {
                    state.var_read(srcs);
                }
                StacklessBytecode::Load(_, dst, _) => {
                    state.var_write(&vec![*dst]);
                }

                StacklessBytecode::Branch(_, _, _, src)
                | StacklessBytecode::Abort(_, src)
                | StacklessBytecode::VariantSwitch(_, src, _) => {
                    state.var_read(&vec![*src]);
                }

                StacklessBytecode::Jump(..)
                | StacklessBytecode::Label(..)
                | StacklessBytecode::Nop(..) => {}
            }

            if let StacklessBytecode::Abort(..) | StacklessBytecode::Ret(..) = inst.bytecode {
                state.terminate();
            }
        }

        use super::super::cfg::datastructs::Terminator;
        if matches!(
            basic_block.inner().next,
            Terminator::Break { .. } | Terminator::Continue { .. }
        ) {
            state.add_break_snapshot();
        }

        Ok(state)
    }
}
