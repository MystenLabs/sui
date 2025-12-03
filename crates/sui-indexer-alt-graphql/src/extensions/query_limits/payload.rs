// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

use std::collections::BTreeSet;

use async_graphql::{
    Name, Pos, Positioned, ServerResult, Variables, parser::types::ExecutableDocument,
    registry::Registry,
};
use async_graphql_value::{ConstValue, Value};
use serde::{Deserialize, Serialize};

use crate::extensions::query_limits::ErrorKind;

use super::{
    QueryLimitsConfig,
    error::Error,
    visitor::{Driver, FieldDriver, Visitor},
};

/// The proportion of the query content that was transaction payload versus query payload.
#[derive(Serialize, Deserialize)]
pub(super) struct Usage {
    pub(super) query_payload_size: u32,
    pub(super) tx_payload_size: u32,
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

        macro_rules! debit {
            ($debit: expr) => {{
                let debit: usize = $debit;
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
            }};
        }

        let mut stack = vec![&value.node];
        while let Some(v) = stack.pop() {
            match v {
                V::Variable(name) => self.check_tx_var(driver, value.pos, name)?,

                V::String(s) => {
                    // Pay for the string, plus the quotes around it.
                    debit!(2 + s.len());
                }

                V::List(vs) => {
                    // Pay for the opening and closing brackets and every comma up-front so that
                    // deeply nested lists are not free.
                    debit!(2 + vs.len().saturating_sub(1));
                    stack.extend(vs);
                }

                V::Object(fs) => {
                    // Pay for the opening and closing braces, colons, commas, and field names
                    // up-front so that deeply nested objects are not free.
                    debit!(
                        2 // { and }
                        +   fs.len().saturating_sub(1) // commas
                        +   fs.keys().map(|k| k.as_str().len() + 1).sum::<usize>() // keys, colons
                    );

                    stack.extend(fs.values());
                }

                V::Number(n) => {
                    // Estimate the string representation of the number.
                    debit!(n.to_string().len());
                }

                V::Boolean(b) => {
                    debit!(if *b { "true".len() } else { "false".len() });
                }

                V::Enum(name) => {
                    // Pay for the enum name.
                    debit!(name.len());
                }

                V::Binary(bs) => {
                    // Pay for the Base64-encoded representation with quotes. Base64 encoding:
                    // every 3 bytes becomes 4 characters.
                    debit!(2 + bs.len().div_ceil(3) * 4);
                }

                V::Null => {
                    debit!("null".len());
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

        macro_rules! debit {
            ($debit: expr) => {{
                let debit: usize = $debit;
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
            }};
        }

        let mut stack = vec![value];
        while let Some(value) = stack.pop() {
            // The cases below are the same as those in `check_tx_arg`, but without having to
            // handle variables and with no bare identifiers (all identifiers -- field names and
            // enums -- are quoted).
            match &value {
                // Pay for the string, plus the quotes around it.
                CV::String(s) => {
                    debit!(2 + s.len());
                }

                // Pay for the opening and closing brackets and every comma up-front so that deeply
                // nested lists are not free.
                CV::List(vs) => {
                    debit!(2 + vs.len().saturating_sub(1));
                    stack.extend(vs);
                }

                // Pay for the opening and closing braces, colons, commas, and field names (with
                // quotes) up-front so that deeply nested objects are not free.
                CV::Object(fs) => {
                    debit!(
                        2 // { and }
                        +   fs.len().saturating_sub(1) // commas
                        +   fs.keys().map(|k| k.as_str().len() + 3).sum::<usize>() // keys, quotes, colons
                    );

                    stack.extend(fs.values());
                }

                CV::Number(n) => {
                    debit!(n.to_string().len());
                }

                CV::Boolean(b) => {
                    debit!(if *b { "true".len() } else { "false".len() });
                }

                // Pay for the enum name, with quotes.
                CV::Enum(name) => {
                    debit!(2 + name.as_str().len());
                }

                CV::Binary(bs) => {
                    debit!(2 + bs.len().div_ceil(3) * 4);
                }

                CV::Null => {
                    debit!("null".len());
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
) -> ServerResult<Usage> {
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
        Err(Error::new_global(ErrorKind::PayloadSizeQuery {
            limit: limits.max_query_payload_size,
            actual: query_payload_size,
        }))?;
    }

    Ok(Usage {
        query_payload_size,
        tx_payload_size,
    })
}
