// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

use super::error::Error;

/// Limits that the parser enforces while parsing potentially multiple Display formats.
pub(crate) struct Limits {
    /// Maximum number of times the parser can recurse into nested structures. Depth does not
    /// account for all nodes, only nodes that can be contained within themselves.
    pub max_depth: usize,

    /// Maximum number of AST nodes that can be allocated during parsing. This counts all values
    /// that are instances of AST types (but not, for example, `Vec<T>`).
    pub max_nodes: usize,

    /// Maximum number of times the format can try to load an object.
    pub max_loads: usize,
}

/// The available budget left for limits that are tracked across all invocations to the parser for
/// a single Display.
pub(crate) struct Budget {
    pub nodes: usize,
    pub loads: usize,
}

pub(crate) struct Meter<'b> {
    depth_budget: usize,
    budget: &'b mut Budget,
}

impl Limits {
    pub fn budget(&self) -> Budget {
        Budget {
            nodes: self.max_nodes,
            loads: self.max_loads,
        }
    }
}

impl<'b> Meter<'b> {
    pub fn new(max_depth: usize, budget: &'b mut Budget) -> Self {
        Meter {
            depth_budget: max_depth,
            budget,
        }
    }

    /// Create a nested meter, with a reduced depth budget.
    pub fn nest(&mut self) -> Result<Meter<'_>, Error> {
        if self.depth_budget == 0 {
            return Err(Error::TooDeep);
        }

        Ok(Meter {
            depth_budget: self.depth_budget - 1,
            budget: self.budget,
        })
    }

    /// Signal that a node has been allocated.
    pub fn alloc(&mut self) -> Result<(), Error> {
        if self.budget.nodes == 0 {
            return Err(Error::TooBig);
        }

        self.budget.nodes -= 1;
        Ok(())
    }

    /// Signal that a load could be performed.
    pub fn load(&mut self) -> Result<(), Error> {
        if self.budget.loads == 0 {
            return Err(Error::TooManyLoads);
        }

        self.budget.loads -= 1;
        Ok(())
    }
}

impl Default for Limits {
    fn default() -> Self {
        Self {
            max_depth: 32,
            max_nodes: 32768,
            max_loads: 8,
        }
    }
}
