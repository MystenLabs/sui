// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

use move_core_types::language_storage::{StructTag, TypeTag};

use crate::{Error, Result};

/// Default limit on cumulative layout and type-tag construction work.
pub const DEFAULT_MAX_TYPE_RESOLUTION_WORK: usize = 10_000;

/// Optional configuration that imposes limits on the work that the resolver can do for each
/// request.
#[derive(Debug, Clone)]
pub struct Limits {
    /// Maximum recursion depth through type parameters.
    pub max_type_argument_depth: usize,
    /// Maximum number of type arguments in a single type instantiation.
    pub max_type_argument_width: usize,
    /// Maximum size for the resolution context and for each type tag.
    pub max_type_nodes: usize,
    /// Maximum recursion depth through struct fields.
    pub max_move_value_depth: usize,
    /// Maximum cumulative number of layout and type-tag nodes constructed or copied.
    pub max_type_resolution_work: usize,
}

/// Tracks cumulative layout and tag work, and supplies the limit for individual tags.
pub(crate) struct LayoutBudget {
    /// The cumulative work limit is absent for unbounded resolution.
    max_work: Option<usize>,
    /// The remaining work is shared across temporary layouts, fields, and type arguments.
    remaining_work: usize,
    /// Each tag is checked independently against this limit; `None` disables that check.
    max_tag_nodes: Option<usize>,
}

/// Counts nodes in one complete type tag, including all nested arguments.
/// A fresh counter is used for each tag, independently of cumulative resolution work.
pub(crate) struct TagBudget {
    /// The per-tag node limit is absent for unbounded resolution.
    max_nodes: Option<usize>,
    /// Number of nodes counted in this tag so far, including the full size of any
    /// type argument checked before cloning it into the tag.
    nodes: usize,
}

/// Tracks path-local value-layout depth while borrowing the shared resolution budget.
pub(crate) struct LayoutMeter<'b> {
    /// The value-layout depth limit is absent for unbounded resolution.
    max_depth: Option<usize>,
    /// The remaining depth decreases for children without affecting sibling paths.
    remaining_depth: usize,
    /// All child meters consume work from the same budget.
    budget: &'b mut LayoutBudget,
}

impl Limits {
    pub(crate) fn layout_budget(&self) -> LayoutBudget {
        LayoutBudget {
            max_work: Some(self.max_type_resolution_work),
            remaining_work: self.max_type_resolution_work,
            max_tag_nodes: Some(self.max_type_nodes),
        }
    }
}

impl LayoutBudget {
    pub(crate) fn unlimited() -> Self {
        Self {
            max_work: None,
            remaining_work: usize::MAX,
            max_tag_nodes: None,
        }
    }

    pub(crate) fn tag_budget(&self) -> TagBudget {
        TagBudget::new(self.max_tag_nodes)
    }

    pub(crate) fn charge_node(&mut self) -> Result<()> {
        self.charge_nodes(1)
    }

    pub(crate) fn charge_nodes(&mut self, nodes: usize) -> Result<()> {
        let Some(max_work) = self.max_work else {
            return Ok(());
        };
        if nodes > self.remaining_work {
            return Err(Error::TooManyTypeNodes(
                max_work,
                max_work
                    .saturating_sub(self.remaining_work)
                    .saturating_add(nodes),
            ));
        }

        self.remaining_work = self.remaining_work.saturating_sub(nodes);
        Ok(())
    }

    pub(crate) fn charge_tag(&mut self, tag: &TypeTag) -> Result<usize> {
        let mut tag_budget = self.tag_budget();
        let mut nodes = 0usize;
        let mut pending = vec![tag];
        while let Some(tag) = pending.pop() {
            tag_budget.charge_nodes(1)?;
            self.charge_node()?;
            nodes = nodes.saturating_add(1);
            match tag {
                TypeTag::Vector(inner) => pending.push(inner),
                TypeTag::Struct(s) => pending.extend(&s.type_params),
                _ => {}
            }
        }
        Ok(nodes)
    }
}

impl TagBudget {
    pub(crate) fn new(max_nodes: Option<usize>) -> Self {
        Self {
            max_nodes,
            nodes: 0,
        }
    }

    pub(crate) fn charge_nodes(&mut self, nodes: usize) -> Result<()> {
        self.nodes = self.nodes.saturating_add(nodes);
        if let Some(max_nodes) = self.max_nodes
            && self.nodes > max_nodes
        {
            return Err(Error::TooManyTypeNodes(max_nodes, self.nodes));
        }
        Ok(())
    }

    pub(crate) fn check_struct_tag(&mut self, tag: &StructTag) -> Result<()> {
        self.charge_nodes(1)?;
        for param in &tag.type_params {
            self.check_tag(param)?;
        }
        Ok(())
    }

    pub(crate) fn check_tag(&mut self, tag: &TypeTag) -> Result<()> {
        let mut pending = vec![tag];
        while let Some(tag) = pending.pop() {
            self.charge_nodes(1)?;
            match tag {
                TypeTag::Vector(inner) => pending.push(inner),
                TypeTag::Struct(s) => pending.extend(&s.type_params),
                _ => {}
            }
        }
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

    pub(crate) fn budget(&mut self) -> &mut LayoutBudget {
        self.budget
    }
}
