// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

use async_graphql::{
    parser::types::ExecutableDocument, registry::Registry, ServerResult, Value, Variables,
};
use serde::{Deserialize, Serialize};

use crate::pagination::{is_connection, PaginationConfig};

use super::{
    error::{Error, ErrorKind},
    visitor::{Driver, FieldDriver, Visitor},
    QueryLimitsConfig,
};

/// How many output nodes are estimated to be output from this query.
#[derive(Serialize, Deserialize)]
pub(super) struct Usage {
    pub nodes: u32,
}

struct OutputNodeBudget<'c> {
    /// The maximum number of output nodes that can be output from a query.
    max_output_nodes: u32,

    /// The number of additional output nodes this query is allowed to output.
    output_node_budget: u32,

    /// Configuration for default page sizes.
    pagination_config: &'c PaginationConfig,
}

/// Validation rule that estimates the maximum number of output nodes the query will produce. It
/// accounts for the fact that paginated and multi-get fields will produce multiple output nodes
/// for each of their recursive input nodes, and assumes that they will always produce the maximal
/// number of output nodes given their arguments (page size or number of keys).
///
/// Note that input nodes that result in lists being output will treat the list as an output node,
/// and then every element as its own output node.
struct OutputNodeRule<'r, 'c> {
    budget: &'r mut OutputNodeBudget<'c>,

    /// An estimate of how many times the query rooted at a given field will be evaluated (any
    /// estimate of how many fields this query will produce needs to be multiplied by this factor).
    multiplicity: u32,

    /// Set when recursing into a paginated field to use as the multiplicative factor when
    /// recursing into its `nodes` or `edges` field.
    page_size: Option<u32>,
}

impl<'r> OutputNodeRule<'r, '_> {
    /// Try to deduct `amount` from the output budget.
    fn deduct(&mut self, driver: &FieldDriver<'_, 'r>, amount: u32) -> Result<(), Error> {
        if amount > self.budget.output_node_budget {
            return Err(driver.err(ErrorKind::OutputNodes(self.budget.max_output_nodes)));
        } else {
            self.budget.output_node_budget -= amount;
        }
        Ok(())
    }

    /// Returns the page size implied by the current field's arguments, assuming it is a paginated
    /// field.
    ///
    /// - If the field is not paginated, returns `None`,
    /// - If the page size exceeds the configured max page size, returns an error.
    /// - If the page size would cause the estimated output node size to overflow, returns an
    ///   error.
    ///
    /// If the field does not specify a page size, a default page size is fetched from the config,
    /// based on the parent type and field name.
    fn page_size(&self, driver: &FieldDriver<'_, 'r>) -> Result<Option<u32>, Error> {
        if !is_connection(driver.meta_field()) {
            return Ok(None);
        }

        let first = self.size_arg(driver, "first")?;
        let last = self.size_arg(driver, "last")?;

        let type_ = driver.parent_type().name();
        let name = driver.meta_field().name.as_str();
        let limits = self.budget.pagination_config.limits(type_, name);

        let size = match (first, last) {
            (Some(f), Some(l)) => f.max(l),
            (Some(p), _) | (_, Some(p)) => p,
            (None, None) => limits.default as u64,
        };

        if size > limits.max as u64 {
            return Err(driver.err(ErrorKind::PageSizeTooLarge {
                limit: limits.max,
                actual: size,
            }));
        }

        // SAFETY: `size <= limits.max <= u32::MAX`.
        Ok(Some(size as u32))
    }

    /// Look for an argument on the current field with the name `name`, and assume that it is a
    /// numeric argument. If the argument is not present, or is not a number, returns `None`.
    fn size_arg(&self, driver: &FieldDriver<'_, 'r>, name: &str) -> Result<Option<u64>, Error> {
        let Some(Value::Number(num)) = driver.resolve_arg(name)? else {
            return Ok(None);
        };

        Ok(num.as_u64())
    }

    /// Returns the number of keys that will be fetched by the current field, assuming it is a
    /// multi-get field (the name starts with `multiGet`, and it contains a `keys` argument that
    /// is a list).
    fn multi_get_size(&self, driver: &FieldDriver<'_, 'r>) -> Result<Option<u32>, Error> {
        if !driver.meta_field().name.starts_with("multiGet") {
            return Ok(None);
        }

        if let Ok(Some(Value::List(vs))) = driver.resolve_arg("keys") {
            let keys = vs.len();
            let limit = self.budget.pagination_config.max_multi_get_size();
            if keys > limit as usize {
                return Err(driver.err(ErrorKind::MultiGetTooLarge {
                    limit,
                    actual: keys,
                }));
            }

            // SAFETY: `keys < limit <= u32::MAX`.
            Ok(Some(keys as u32))
        } else {
            Ok(None)
        }
    }
}

impl<'r> Visitor<'r> for OutputNodeRule<'r, '_> {
    fn visit_field(&mut self, driver: &FieldDriver<'_, 'r>) -> Result<(), Error> {
        let mut multiplicity = self.multiplicity;
        let mut page_size = self.page_size;

        // Start by deducting the cost of the current field.
        self.deduct(driver, multiplicity)?;

        if ["nodes", "edges"].contains(&driver.meta_field().name.as_str()) {
            // If the current field looks like a page content field for a Connection, and there is
            // a page size set (meaning this field is nested within a Connection field), then
            // nested fields will be multiplied by the connection's page size.
            if let Some(size) = page_size {
                multiplicity = self.multiplicity.saturating_mul(size);
                page_size = None;

                // Deduct the cost of an output node per element of the page.
                self.deduct(driver, multiplicity)?;
            }
        } else if let Some(size) = self.multi_get_size(driver)? {
            // If the current field is a multi-get, then the multiplicity increases and we know any
            // page size that was set previously is no longer relevant.
            multiplicity = self.multiplicity.saturating_mul(size);
            page_size = None;

            // Deduct the cost of an output node per element of the page.
            self.deduct(driver, multiplicity)?;
        } else if let Some(size) = self.page_size(driver)? {
            // If the current field is a Connection, its arguments determine the page size that
            // impacts nested fields.
            page_size = Some(size);
        }

        driver.visit_selection_set(&mut OutputNodeRule {
            budget: self.budget,
            multiplicity,
            page_size,
        })
    }
}

/// Test that the the query does not produce an excessively large output by estimating the number
/// of output nodes it will produce before executing it.
///
/// This check must be done after the input limit check, because it relies on the query depth being
/// bounded to protect it from recursing too deeply.
pub(super) fn check(
    limits: &QueryLimitsConfig,
    pagination_config: &PaginationConfig,
    registry: &Registry,
    doc: &ExecutableDocument,
    variables: &Variables,
) -> ServerResult<Usage> {
    let mut budget = OutputNodeBudget {
        max_output_nodes: limits.max_output_nodes,
        output_node_budget: limits.max_output_nodes,
        pagination_config,
    };

    Driver::visit_document(
        registry,
        doc,
        variables,
        &mut OutputNodeRule {
            budget: &mut budget,
            multiplicity: 1,
            page_size: None,
        },
    )?;

    Ok(Usage {
        nodes: limits.max_output_nodes - budget.output_node_budget,
    })
}
