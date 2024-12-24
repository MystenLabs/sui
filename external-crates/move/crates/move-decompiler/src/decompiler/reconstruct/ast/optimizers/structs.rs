// Copyright (c) Verichains, 2023

use std::collections::{HashMap, HashSet};

pub(crate) struct VariableRenamingIndexMap {
    vars: HashMap<usize, usize>,
}

impl VariableRenamingIndexMap {
    pub(crate) fn identity(cnt: usize) -> Self {
        let mut vars = HashMap::new();
        for i in 0..cnt {
            vars.insert(i, i);
        }
        Self { vars }
    }

    pub(crate) fn current_variables(&self) -> HashSet<usize> {
        self.vars.keys().cloned().collect()
    }

    pub(crate) fn get(&self, var: usize) -> usize {
        *self.vars.get(&var).unwrap()
    }

    pub(crate) fn apply(&mut self, rename_map: &HashMap<usize, usize>) {
        let from: Vec<_> = rename_map.keys().collect();
        let to: Vec<_> = from.iter().map(|k| *rename_map.get(k).unwrap()).collect();
        let original: Vec<_> = from.iter().map(|&&k| self.get(k)).collect();

        let n = from.len();

        for i in 0..n {
            self.vars.remove(&from[i]);
        }

        for i in 0..n {
            self.vars.insert(to[i], original[i]);
        }
    }
}
