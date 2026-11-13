// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

use crate::{Error, Result};

/// Default limit on the cumulative number of layout nodes constructed or copied.
pub const DEFAULT_MAX_LAYOUT_WORK: usize = 2_048;

/// Optional configuration that imposes limits on the work that the resolver can do for each
/// request.
#[derive(Debug, Clone)]
pub struct Limits {
    /// Maximum recursion depth through type parameters.
    pub max_type_argument_depth: usize,
    /// Maximum number of type arguments in a single type instantiation.
    pub max_type_argument_width: usize,
    /// Maximum size for the resolution context.
    pub max_type_nodes: usize,
    /// Maximum recursion depth through struct fields.
    pub max_move_value_depth: usize,
    /// Maximum cumulative number of layout nodes constructed or copied during resolution.
    pub max_layout_work: usize,
}

pub(crate) struct LayoutBudget {
    max_nodes: Option<usize>,
    remaining_nodes: usize,
}

pub(crate) struct LayoutMeter<'b> {
    max_depth: Option<usize>,
    remaining_depth: usize,
    budget: &'b mut LayoutBudget,
}

impl Limits {
    pub(crate) fn layout_budget(&self) -> LayoutBudget {
        LayoutBudget {
            max_nodes: Some(self.max_layout_work),
            remaining_nodes: self.max_layout_work,
        }
    }
}

impl LayoutBudget {
    pub(crate) fn unlimited() -> Self {
        Self {
            max_nodes: None,
            remaining_nodes: usize::MAX,
        }
    }

    fn charge_node(&mut self) -> Result<()> {
        self.charge_nodes(1)
    }

    fn charge_nodes(&mut self, nodes: usize) -> Result<()> {
        let Some(max_nodes) = self.max_nodes else {
            return Ok(());
        };
        if nodes > self.remaining_nodes {
            return Err(Error::TooManyTypeNodes(
                max_nodes,
                max_nodes
                    .saturating_sub(self.remaining_nodes)
                    .saturating_add(nodes),
            ));
        }

        self.remaining_nodes = self.remaining_nodes.saturating_sub(nodes);
        Ok(())
    }
}

impl<'b> LayoutMeter<'b> {
    pub(crate) fn new(max_depth: Option<usize>, budget: &'b mut LayoutBudget) -> Self {
        Self {
            max_depth,
            remaining_depth: max_depth.unwrap_or(usize::MAX),
            budget,
        }
    }

    pub(crate) fn check_depth(&self) -> Result<()> {
        if self.remaining_depth == 0 {
            return Err(Error::ValueNesting(self.max_depth.unwrap_or(0)));
        }
        Ok(())
    }

    pub(crate) fn ensure_depth(&self, depth: usize) -> Result<()> {
        if depth > self.remaining_depth {
            return Err(Error::ValueNesting(self.max_depth.unwrap_or(0)));
        }
        Ok(())
    }

    pub(crate) fn nest(&mut self) -> Result<LayoutMeter<'_>> {
        self.check_depth()?;
        Ok(LayoutMeter {
            max_depth: self.max_depth,
            remaining_depth: self.remaining_depth.saturating_sub(1),
            budget: self.budget,
        })
    }

    pub(crate) fn charge_node(&mut self) -> Result<()> {
        self.budget.charge_node()
    }

    pub(crate) fn charge_nodes(&mut self, nodes: usize) -> Result<()> {
        self.budget.charge_nodes(nodes)
    }
}
