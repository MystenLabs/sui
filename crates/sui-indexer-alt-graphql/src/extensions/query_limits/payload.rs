// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

use std::collections::BTreeSet;

use async_graphql::{
    parser::types::ExecutableDocument, registry::Registry, Name, Pos, Positioned, Variables,
};
use async_graphql_value::{ConstValue, Value};
use serde::{Deserialize, Serialize};

use crate::extensions::query_limits::ErrorKind;

use super::{
    error::Error,
    visitor::{Driver, FieldDriver, Visitor},
    QueryLimitsConfig,
};

/// The proportion of the query content that was transaction payload versus query payload.
#[derive(Serialize, Deserialize)]
pub(super) struct Usage {
    pub query_payload_size: u32,
    pub tx_payload_size: u32,
}

/// Validation rule that finds parts of the request that correspond to transaction payloads
/// (transaction bytes, signatures, etc), and deducts them from an overall budget.
///
/// NOTE: This rule only checks top-level fields (it does not look for transaction payloads in
/// nested structures) as currently transaction payloads can only be found there.
struct TxPayloadRule<'r> {
    /// The maximum number of bytes in a request that can come from transaction payloads.
    max_tx_payload_size: u32,

    /// The budget for transaction payloads remaining.
    tx_payload_budget: u32,

    /// Names of variables that have been used in transaction payloads -- variables that show up in
    /// transaction payloads multiple times are only debited from the budget once.
    used_vars: BTreeSet<&'r Name>,

    /// Set of type, field, and argument triples that are known to contain transaction payloads.
    tx_payload_args: &'r BTreeSet<(&'static str, &'static str, &'static str)>,
}

impl<'r> TxPayloadRule<'r> {
    /// Whether the type and field pointed at currently by `driver` contains any transaction
    /// payload arguments or not.
    fn has_tx_payload(&self, driver: &FieldDriver<'_, 'r>) -> bool {
        let type_ = driver.parent_type().name();
        let field = driver.meta_field().name.as_str();

        self.tx_payload_args
            .range(&(type_, field, "")..)
            .next()
            .is_some_and(|&(t, f, _)| t == type_ && f == field)
    }

    /// Debit the size of `value` assuming `name` is a transaction payload argument of the type and
    /// field pointed at by `driver`.
    ///
    /// Fails if there is insufficient remaining budget.
    fn check_tx_arg(
        &mut self,
        driver: &FieldDriver<'_, 'r>,
        name: &'r Positioned<Name>,
        value: &'r Positioned<Value>,
    ) -> Result<(), Error> {
        use Value as V;

        let type_ = driver.parent_type().name();
        let field = driver.meta_field().name.as_str();
        let arg = name.node.as_str();

        if !self.tx_payload_args.contains(&(type_, field, arg)) {
            return Ok(());
        }

        let mut stack = vec![&value.node];
        while let Some(v) = stack.pop() {
            match v {
                V::Variable(name) => self.check_tx_var(driver, value.pos, name)?,

                V::String(s) => {
                    // Pay for the string, plus the quotes around it.
                    let debit = s.len() + 2;
                    if debit > self.tx_payload_budget as usize {
                        return Err(driver.err_at(
                            value.pos,
                            ErrorKind::PayloadSizeTx {
                                limit: self.max_tx_payload_size,
                            },
                        ));
                    } else {
                        // SAFETY: We know that debit <= self.tx_payload_budget, which is a u32, so
                        // the cast and subtraction are both safe.
                        self.tx_payload_budget -= debit as u32;
                    }
                }

                V::List(vs) => {
                    // Pay for the opening and closing brackets and every comma up-front so that
                    // deeply nested lists are not free.
                    let debit = vs.len().saturating_sub(1) + 2;
                    if debit > self.tx_payload_budget as usize {
                        return Err(driver.err_at(
                            value.pos,
                            ErrorKind::PayloadSizeTx {
                                limit: self.max_tx_payload_size,
                            },
                        ));
                    } else {
                        // SAFETY: We know that debit <= self.tx_payload_budget, which is a u32, so
                        // the cast and subtraction are both safe.
                        self.tx_payload_budget -= debit as u32;
                        stack.extend(vs)
                    }
                }

                V::Null
                | V::Number(_)
                | V::Boolean(_)
                | V::Binary(_)
                | V::Enum(_)
                | V::Object(_) => {
                    // Transaction payloads cannot be any of these types, so this request is
                    // destined to fail. Ignore these values for now, so that it can fail later on
                    // with a more legible error message.
                    //
                    // From a limits perspective, it is safe to ignore these values here, because
                    // they will still be counted as part of the query payload (and so are still
                    // subject to a limit).
                }
            }
        }

        Ok(())
    }

    /// Debit the size of the value for the variable `name` (assuming it is used in a transaction
    /// payload argument), if it has not already been used in a transaction payload (in which case
    /// it is already accounted for).
    ///
    /// Fails if there is insufficient remaining budget.
    fn check_tx_var(
        &mut self,
        driver: &FieldDriver<'_, 'r>,
        pos: Pos,
        name: &'r Name,
    ) -> Result<(), Error> {
        use ConstValue as CV;

        // Already used in a transaction payload, don't double count.
        if !self.used_vars.insert(name) {
            return Ok(());
        }

        // Can't find the variable, so it can't count towards the transaction payload.
        let Some(value) = driver.resolve_var(name) else {
            return Ok(());
        };

        let mut stack = vec![value];
        while let Some(value) = stack.pop() {
            match &value {
                CV::String(s) => {
                    // Pay for the string, plus the quotes around it.
                    let debit = s.len() + 2;
                    if debit > self.tx_payload_budget as usize {
                        return Err(driver.err_at(
                            pos,
                            ErrorKind::PayloadSizeTx {
                                limit: self.max_tx_payload_size,
                            },
                        ));
                    } else {
                        // SAFETY: We know that debit <= self.tx_payload_budget, which is a u32, so
                        // the cast and subtraction are both safe.
                        self.tx_payload_budget -= debit as u32;
                    }
                }

                CV::List(vs) => {
                    // Pay for the opening and closing brackets and every comma up-front so that
                    // deeply nested lists are not free.
                    let debit = vs.len().saturating_sub(1) + 2;
                    if debit > self.tx_payload_budget as usize {
                        return Err(driver.err_at(
                            pos,
                            ErrorKind::PayloadSizeTx {
                                limit: self.max_tx_payload_size,
                            },
                        ));
                    } else {
                        // SAFETY: We know that debit <= self.tx_payload_budget, which is a u32, so
                        // the cast and subtraction are both safe.
                        self.tx_payload_budget -= debit as u32;
                        stack.extend(vs)
                    }
                }

                CV::Null
                | CV::Number(_)
                | CV::Boolean(_)
                | CV::Binary(_)
                | CV::Enum(_)
                | CV::Object(_) => {
                    // As in `check_tx_arg`, these are safe to ignore.
                }
            }
        }

        Ok(())
    }
}

impl<'r> Visitor<'r> for TxPayloadRule<'r> {
    fn visit_field(&mut self, driver: &FieldDriver<'_, 'r>) -> Result<(), Error> {
        if !self.has_tx_payload(driver) {
            return Ok(());
        }

        for (name, value) in &driver.field().node.arguments {
            self.check_tx_arg(driver, name, value)?;
        }

        Ok(())
    }
}

/// Test that arguments holding transaction payloads (serialized transactions, signatures, etc) are
/// within the service's transaction payload limit, cumulatively.
///
/// Similarly, test that the remaining transaction payload is within the query payload limit (which
/// is usually smaller).
///
/// This check must be done after the input limit check, because it relies on the query depth being
/// bounded to protect it from recursing too deeply.
pub(super) fn check(
    limits: &QueryLimitsConfig,
    total_payload_size: u64,
    registry: &Registry,
    doc: &ExecutableDocument,
    variables: &Variables,
) -> Result<Usage, Error> {
    let mut rule = TxPayloadRule {
        max_tx_payload_size: limits.max_tx_payload_size,
        tx_payload_budget: limits.max_tx_payload_size,
        used_vars: BTreeSet::new(),
        tx_payload_args: &limits.tx_payload_args,
    };

    Driver::visit_document(registry, doc, variables, &mut rule)?;

    let tx_payload_size = limits.max_tx_payload_size - rule.tx_payload_budget;
    let query_payload_size = total_payload_size as u32 - tx_payload_size;

    if query_payload_size > limits.max_query_payload_size {
        return Err(Error::new_global(ErrorKind::PayloadSizeQuery {
            limit: limits.max_query_payload_size,
            actual: query_payload_size,
        }));
    }

    Ok(Usage {
        query_payload_size,
        tx_payload_size,
    })
}
