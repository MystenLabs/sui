// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

use crate::config::{Limits, ServiceConfig};
use crate::error::{code, graphql_error, graphql_error_at_pos};
use crate::metrics::Metrics;
use async_graphql::extensions::NextParseQuery;
use async_graphql::extensions::NextRequest;
use async_graphql::extensions::{Extension, ExtensionContext, ExtensionFactory};
use async_graphql::parser::types::{
    DocumentOperations, ExecutableDocument, Field, FragmentDefinition, OperationDefinition,
    Selection,
};
use async_graphql::{value, Name, Pos, Positioned, Response, ServerError, ServerResult, Variables};
use async_graphql_value::Value as GqlValue;
use async_graphql_value::{ConstValue, Value};
use async_trait::async_trait;
use axum::http::HeaderName;
use serde::Serialize;
use std::collections::{HashMap, HashSet};
use std::mem;
use std::net::SocketAddr;
use std::sync::{Arc, Mutex};
use std::time::Instant;
use sui_graphql_rpc_headers::LIMITS_HEADER;
use tracing::{error, info};
use uuid::Uuid;

pub(crate) const CONNECTION_FIELDS: [&str; 2] = ["edges", "nodes"];
const DRY_RUN_TX_BLOCK: &str = "dryRunTransactionBlock";
const EXECUTE_TX_BLOCK: &str = "executeTransactionBlock";
const MULTI_GET_PREFIX: &str = "multiGet";
const MULTI_GET_KEYS: &str = "keys";
const VERIFY_ZKLOGIN: &str = "verifyZkloginSignature";

/// The size of the query payload in bytes, as it comes from the request header: `Content-Length`.
#[derive(Clone, Copy, Debug)]
pub(crate) struct PayloadSize(pub u64);

/// Extension factory for adding checks that the query is within configurable limits.
pub(crate) struct QueryLimitsChecker;

#[derive(Debug, Default)]
struct QueryLimitsCheckerExt {
    usage: Mutex<Option<Usage>>,
}

/// Only display usage information if this header was in the request.
pub(crate) struct ShowUsage;

/// State for traversing a document to check for limits. Holds on to environments for looking up
/// variables and fragments, limits, and the remainder of the limit that can be used.
struct LimitsTraversal<'a> {
    // Environments for resolving lookups in the document
    fragments: &'a HashMap<Name, Positioned<FragmentDefinition>>,
    variables: &'a Variables,

    /// Creates and trace errors
    reporter: &'a Reporter<'a>,

    /// Raw size of the request
    payload_size: u64,

    /// Variables that are used in transaction executions and dry-runs. If these variables are used
    /// multiple times, the size of their contents should not be double counted.
    tx_variables_used: HashSet<&'a Name>,

    // Remaining budget for the traversal
    tx_payload_budget: u32,
    input_budget: u32,
    output_budget: u32,
    depth_seen: u32,
}

/// Builds error messages and reports them to tracing.
struct Reporter<'a> {
    limits: &'a Limits,
    query_id: &'a Uuid,
    session_id: &'a SocketAddr,
}

#[derive(Clone, Debug, Default, Serialize)]
#[serde(rename_all = "camelCase")]
struct Usage {
    input_nodes: u32,
    output_nodes: u32,
    depth: u32,
    variables: u32,
    fragments: u32,
    query_payload: u32,
}

impl ShowUsage {
    pub(crate) fn name() -> &'static HeaderName {
        &LIMITS_HEADER
    }
}

impl<'a> LimitsTraversal<'a> {
    fn new(
        PayloadSize(payload_size): PayloadSize,
        reporter: &'a Reporter<'a>,
        fragments: &'a HashMap<Name, Positioned<FragmentDefinition>>,
        variables: &'a Variables,
    ) -> Self {
        Self {
            fragments,
            variables,
            payload_size,
            reporter,
            tx_variables_used: HashSet::new(),
            tx_payload_budget: reporter.limits.max_tx_payload_size,
            input_budget: reporter.limits.max_query_nodes,
            output_budget: reporter.limits.max_output_nodes,
            depth_seen: 0,
        }
    }

    /// Main entrypoint for checking all limits.
    fn check_document(&mut self, doc: &'a ExecutableDocument) -> ServerResult<()> {
        // First, check the size of the query inputs. This is done using a non-recursive algorithm in
        // case the input has too many nodes or is too deep. This allows subsequent checks to be
        // implemented recursively.
        for (_name, op) in doc.operations.iter() {
            self.check_input_limits(op)?;
        }

        // Then gather inputs to transaction execution and dry-run nodes, and make sure these are
        // within budget, cumulatively.
        for (_name, op) in doc.operations.iter() {
            self.check_tx_payload(op)?;
        }

        // Next, with the transaction payloads accounted for, ensure the remaining query is within
        // the size limit.
        let limits = self.reporter.limits;
        let tx_payload_size = (limits.max_tx_payload_size - self.tx_payload_budget) as u64;
        let query_payload_size = self.payload_size - tx_payload_size;
        if query_payload_size > limits.max_query_payload_size as u64 {
            let message = format!("Query part too large: {query_payload_size} bytes");
            return Err(self.reporter.payload_size_error(&message));
        }

        // Finally, run output node estimation, to check that the output won't contain too many
        // nodes, in the worst case.
        for (_name, op) in doc.operations.iter() {
            self.check_output_limits(op)?;
        }

        Ok(())
    }

    /// Test that the operation meets input limits (number of nodes and depth).
    fn check_input_limits(&mut self, op: &Positioned<OperationDefinition>) -> ServerResult<()> {
        let limits = self.reporter.limits;

        let mut next_level = vec![];
        let mut curr_level = vec![];
        let mut depth_budget = limits.max_query_depth;

        next_level.extend(&op.node.selection_set.node.items);
        while let Some(next) = next_level.first() {
            if depth_budget == 0 {
                return Err(self.reporter.graphql_error_at_pos(
                    code::BAD_USER_INPUT,
                    format!("Query nesting is over {}", limits.max_query_depth),
                    next.pos,
                ));
            } else {
                depth_budget -= 1;
            }

            mem::swap(&mut next_level, &mut curr_level);

            for selection in curr_level.drain(..) {
                if self.input_budget == 0 {
                    return Err(self.reporter.graphql_error_at_pos(
                        code::BAD_USER_INPUT,
                        format!("Query has over {} nodes", limits.max_query_nodes),
                        selection.pos,
                    ));
                } else {
                    self.input_budget -= 1;
                }

                match &selection.node {
                    Selection::Field(f) => {
                        next_level.extend(&f.node.selection_set.node.items);
                    }

                    Selection::InlineFragment(f) => {
                        next_level.extend(&f.node.selection_set.node.items);
                    }

                    Selection::FragmentSpread(fs) => {
                        let name = &fs.node.fragment_name.node;
                        let def = self
                            .fragments
                            .get(name)
                            .ok_or_else(|| self.reporter.fragment_not_found_error(name, fs.pos))?;

                        next_level.extend(&def.node.selection_set.node.items);
                    }
                }
            }
        }

        self.depth_seen = self.depth_seen.max(limits.max_query_depth - depth_budget);
        Ok(())
    }

    /// Test that inputs to `executeTransactionBlock` and `dryRunTransactionBlock` take up less
    /// space than the service's transaction payload limit, cumulatively.
    ///
    /// This check must be done after the input limit check, because it relies on the query depth
    /// being bounded to protect it from recursing too deeply.
    fn check_tx_payload(&mut self, op: &'a Positioned<OperationDefinition>) -> ServerResult<()> {
        for item in &op.node.selection_set.node.items {
            self.traverse_selection_for_tx_payload(item)?;
        }
        Ok(())
    }

    /// Look for `executeTransactionBlock` and `dryRunTransactionBlock` nodes among the
    /// query selections, and check their argument sizes are under the service limits.
    fn traverse_selection_for_tx_payload(
        &mut self,
        item: &'a Positioned<Selection>,
    ) -> ServerResult<()> {
        match &item.node {
            Selection::Field(f) => {
                let name = &f.node.name.node;

                if name == DRY_RUN_TX_BLOCK || name == EXECUTE_TX_BLOCK {
                    for (_name, value) in &f.node.arguments {
                        self.check_tx_arg(value)?;
                    }
                } else if name == VERIFY_ZKLOGIN {
                    if let Some(value) = f.node.get_argument("bytes") {
                        self.check_tx_arg(value)?;
                    }

                    if let Some(value) = f.node.get_argument("signature") {
                        self.check_tx_arg(value)?;
                    }
                }
            }

            Selection::InlineFragment(f) => {
                for selection in &f.node.selection_set.node.items {
                    self.traverse_selection_for_tx_payload(selection)?;
                }
            }

            Selection::FragmentSpread(fs) => {
                let name = &fs.node.fragment_name.node;
                let def = self
                    .fragments
                    .get(name)
                    .ok_or_else(|| self.reporter.fragment_not_found_error(name, fs.pos))?;

                for selection in &def.node.selection_set.node.items {
                    self.traverse_selection_for_tx_payload(selection)?;
                }
            }
        }
        Ok(())
    }

    /// Deduct the size of the transaction argument's `value` from the transaction payload budget.
    /// This operation resolves variables and deducts their size from the budget as well, as long
    /// as they have not already been encountered in some previous transaction payload.
    ///
    /// Fails if there is insufficient remaining budget.
    fn check_tx_arg(&mut self, value: &'a Positioned<Value>) -> ServerResult<()> {
        use GqlValue as V;

        let mut stack = vec![&value.node];
        while let Some(value) = stack.pop() {
            match value {
                V::Variable(name) => self.check_tx_var(name)?,

                V::String(s) => {
                    // Pay for the string, plus the quotes around it.
                    let debit = s.len() + 2;
                    if debit > self.tx_payload_budget as usize {
                        return Err(self.tx_payload_size_error());
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
                        return Err(self.tx_payload_size_error());
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

    /// Deduct the size of the value that variable `name` resolve to from the transaction payload
    /// budget, if it has not already been encountered in a previous transaction payload.
    ///
    /// Fails if there is insufficient remaining budget.
    fn check_tx_var(&mut self, name: &'a Name) -> ServerResult<()> {
        use ConstValue as CV;

        // Already used in a transaction, don't double count.
        if !self.tx_variables_used.insert(name) {
            return Ok(());
        }

        // Can't find the variable, so it can't count towards the transaction payload.
        let Some(value) = self.variables.get(name) else {
            return Ok(());
        };

        let mut stack = vec![value];
        while let Some(value) = stack.pop() {
            match &value {
                CV::String(s) => {
                    // Pay for the string, plus the quotes around it.
                    let debit = s.len() + 2;
                    if debit > self.tx_payload_budget as usize {
                        return Err(self.tx_payload_size_error());
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
                        return Err(self.tx_payload_size_error());
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

    /// Check that the operation's output node estimate will not exceed the service's limit.
    ///
    /// This check must be done after the input limit check, because it relies on the query depth
    /// being bounded to protect it from recursing too deeply.
    fn check_output_limits(&mut self, op: &Positioned<OperationDefinition>) -> ServerResult<()> {
        for selection in &op.node.selection_set.node.items {
            self.traverse_selection_for_output(selection, 1, None)?;
        }
        Ok(())
    }

    /// Account for the estimated output size of this selection and its children.
    ///
    /// `multiplicity` is the number of times this selection will be output, on account of being
    /// nested within paginated ancestors.
    ///
    /// If this field is inside a connection, but not inside one of its fields, `page_size` is the
    /// size of the connection's page.
    fn traverse_selection_for_output(
        &mut self,
        selection: &Positioned<Selection>,
        multiplicity: u32,
        page_size: Option<u32>,
    ) -> ServerResult<()> {
        match &selection.node {
            Selection::Field(f) => {
                if multiplicity > self.output_budget {
                    return Err(self.output_node_error());
                } else {
                    self.output_budget -= multiplicity;
                }

                let name = &f.node.name.node;

                // Handle regular connection fields and multiGet queries
                let multiplicity = 'm: {
                    // check if it is a multiGet query and return the number of keys
                    if let Some(page_size) = self.multi_get_page_size(f)? {
                        break 'm multiplicity * page_size;
                    }

                    if !CONNECTION_FIELDS.contains(&name.as_str()) {
                        break 'm multiplicity;
                    }

                    let Some(page_size) = page_size else {
                        break 'm multiplicity;
                    };
                    multiplicity
                        .checked_mul(page_size)
                        .ok_or_else(|| self.output_node_error())?
                };

                let page_size = self.connection_page_size(f)?;
                for selection in &f.node.selection_set.node.items {
                    self.traverse_selection_for_output(selection, multiplicity, page_size)?;
                }
            }

            // Just recurse through fragments, because they are inlined into their "call site".
            Selection::InlineFragment(f) => {
                for selection in &f.node.selection_set.node.items {
                    self.traverse_selection_for_output(selection, multiplicity, page_size)?;
                }
            }

            Selection::FragmentSpread(fs) => {
                let name = &fs.node.fragment_name.node;
                let def = self
                    .fragments
                    .get(name)
                    .ok_or_else(|| self.reporter.fragment_not_found_error(name, fs.pos))?;

                for selection in &def.node.selection_set.node.items {
                    self.traverse_selection_for_output(selection, multiplicity, page_size)?;
                }
            }
        }

        Ok(())
    }

    /// If the field `f` is a connection, extract its page size, otherwise return `None`.
    /// Returns an error if the page size cannot be represented as a `u32`.
    fn connection_page_size(&mut self, f: &Positioned<Field>) -> ServerResult<Option<u32>> {
        if !self.is_connection(f) {
            return Ok(None);
        }

        let first = f.node.get_argument("first");
        let last = f.node.get_argument("last");

        let page_size = match (self.resolve_u64(first), self.resolve_u64(last)) {
            (Some(f), Some(l)) => f.max(l),
            (Some(p), _) | (_, Some(p)) => p,
            (None, None) => self.reporter.limits.default_page_size as u64,
        };

        Ok(Some(
            page_size.try_into().map_err(|_| self.output_node_error())?,
        ))
    }

    // If the field `f` is a multiGet query, extract the number of keys, otherwise return `None`.
    // Returns an error if the number of keys cannot be represented as a `u32`.
    fn multi_get_page_size(&mut self, f: &Positioned<Field>) -> ServerResult<Option<u32>> {
        if !f.node.name.node.starts_with(MULTI_GET_PREFIX) {
            return Ok(None);
        }

        let keys = f.node.get_argument(MULTI_GET_KEYS);
        let Some(page_size) = self.resolve_list_size(keys) else {
            return Ok(None);
        };

        Ok(Some(
            page_size.try_into().map_err(|_| self.output_node_error())?,
        ))
    }

    /// Checks if the given field corresponds to a connection based on whether it contains a
    /// selection for `edges` or `nodes`. That selection could be immediately in that field's
    /// selection set, or nested within a fragment or inline fragment spread.
    fn is_connection(&self, f: &Positioned<Field>) -> bool {
        f.node
            .selection_set
            .node
            .items
            .iter()
            .any(|s| self.has_connection_fields(s))
    }

    /// Look for fields that suggest the container for this selection is a connection. Recurses
    /// through fragment and inline fragment applications, but does not look recursively through
    /// fields, as only the fields requested from the immediate parent are relevant.
    fn has_connection_fields(&self, s: &Positioned<Selection>) -> bool {
        match &s.node {
            Selection::Field(f) => {
                let name = &f.node.name.node;
                CONNECTION_FIELDS.contains(&name.as_str())
            }

            Selection::InlineFragment(f) => f
                .node
                .selection_set
                .node
                .items
                .iter()
                .any(|s| self.has_connection_fields(s)),

            Selection::FragmentSpread(fs) => {
                let name = &fs.node.fragment_name.node;
                let Some(def) = self.fragments.get(name) else {
                    return false;
                };

                def.node
                    .selection_set
                    .node
                    .items
                    .iter()
                    .any(|s| self.has_connection_fields(s))
            }
        }
    }

    /// Translate a GraphQL value into a u64, if possible, resolving variables if necessary.
    fn resolve_u64(&self, value: Option<&Positioned<Value>>) -> Option<u64> {
        match &value?.node {
            Value::Number(num) => num,

            Value::Variable(var) => {
                if let ConstValue::Number(num) = self.variables.get(var)? {
                    num
                } else {
                    return None;
                }
            }

            _ => return None,
        }
        .as_u64()
    }

    /// Find the size of a list, resolving variables if necessary.
    fn resolve_list_size(&self, value: Option<&Positioned<Value>>) -> Option<usize> {
        match &value?.node {
            Value::List(list) => Some(list.len()),
            Value::Variable(var) => {
                if let ConstValue::List(list) = self.variables.get(var)? {
                    Some(list.len())
                } else {
                    None
                }
            }
            _ => None,
        }
    }

    /// Error returned if transaction payloads exceed limit. Also sets the transaction payload
    /// budget to zero to indicate it has been spent (This is done to prevent future checks for
    /// smaller arguments from succeeding even though a previous larger argument has already
    /// failed).
    fn tx_payload_size_error(&mut self) -> ServerError {
        self.tx_payload_budget = 0;
        self.reporter
            .payload_size_error("Transaction payload too large")
    }

    /// Error returned if output node estimate exceeds limit. Also sets the output budget to zero,
    /// to indicate that it has been spent (This is done because unlike other budgets, the output
    /// budget is not decremented one unit at a time, so we can have hit the limit previously but
    /// still have budget left over).
    fn output_node_error(&mut self) -> ServerError {
        self.output_budget = 0;
        self.reporter.output_node_error()
    }

    /// Finish the traversal and report its usage.
    fn finish(self, query_payload: u32) -> Usage {
        let limits = self.reporter.limits;
        Usage {
            input_nodes: limits.max_query_nodes - self.input_budget,
            output_nodes: limits.max_output_nodes - self.output_budget,
            depth: self.depth_seen,
            variables: self.variables.len() as u32,
            fragments: self.fragments.len() as u32,
            query_payload,
        }
    }
}

impl<'a> Reporter<'a> {
    fn new(ctx: &'a ExtensionContext<'a>) -> Self {
        let cfg: &ServiceConfig = ctx.data_unchecked();
        Self {
            limits: &cfg.limits,
            query_id: ctx.data_unchecked(),
            session_id: ctx.data_unchecked(),
        }
    }

    /// Error returned if a fragment is referred to but not found in the document.
    fn fragment_not_found_error(&self, name: &Name, pos: Pos) -> ServerError {
        self.graphql_error_at_pos(
            code::BAD_USER_INPUT,
            format!("Fragment {name} referred to but not found in document"),
            pos,
        )
    }

    /// Error returned if output node estimate exceeds limit.
    fn output_node_error(&self) -> ServerError {
        self.graphql_error(
            code::BAD_USER_INPUT,
            format!(
                "Estimated output nodes exceeds {}",
                self.limits.max_output_nodes
            ),
        )
    }

    /// Error returned if the payload size exceeds the limit.
    fn payload_size_error(&self, message: &str) -> ServerError {
        self.graphql_error(
            code::BAD_USER_INPUT,
            format!(
                "{message}. Requests are limited to {max_tx_payload} bytes or fewer on transaction \
                 payloads (all inputs to executeTransactionBlock, dryRunTransactionBlock, or \
                 verifyZkloginSignature) and the rest of the request (the query part) must be \
                 {max_query_payload} bytes or fewer.",
                max_tx_payload = self.limits.max_tx_payload_size,
                max_query_payload = self.limits.max_query_payload_size,
            ),
        )
    }

    /// Build a GraphQL Server Error and also log it.
    fn graphql_error(&self, code: &str, message: String) -> ServerError {
        self.log_error(code, &message);
        graphql_error(code, message)
    }

    /// Like `graphql_error` but for an error at a specific position in the query.
    fn graphql_error_at_pos(&self, code: &str, message: String, pos: Pos) -> ServerError {
        self.log_error(code, &message);
        graphql_error_at_pos(code, message, pos)
    }

    /// Log an error (used before returning an error response.
    fn log_error(&self, error_code: &str, message: &str) {
        if error_code == code::INTERNAL_SERVER_ERROR {
            error!(
                query_id = %self.query_id,
                session_id = %self.session_id,
                error_code,
                "Internal error while checking limits: {message}",
            );
        } else {
            info!(
                query_id = %self.query_id,
                session_id = %self.session_id,
                error_code,
                "Limits error: {message}",
            );
        }
    }
}

impl Usage {
    fn report(&self, metrics: &Metrics) {
        metrics
            .request_metrics
            .input_nodes
            .observe(self.input_nodes as f64);
        metrics
            .request_metrics
            .output_nodes
            .observe(self.output_nodes as f64);
        metrics
            .request_metrics
            .query_depth
            .observe(self.depth as f64);
        metrics
            .request_metrics
            .query_payload_size
            .observe(self.query_payload as f64);
    }
}

impl ExtensionFactory for QueryLimitsChecker {
    fn create(&self) -> Arc<dyn Extension> {
        Arc::new(QueryLimitsCheckerExt {
            usage: Mutex::new(None),
        })
    }
}

#[async_trait]
impl Extension for QueryLimitsCheckerExt {
    async fn request(&self, ctx: &ExtensionContext<'_>, next: NextRequest<'_>) -> Response {
        let resp = next.run(ctx).await;
        let usage = self.usage.lock().unwrap().take();
        if let Some(usage) = usage {
            resp.extension("usage", value!(usage))
        } else {
            resp
        }
    }

    /// Validates the query against the limits set in the service config
    /// If the limits are hit, the operation terminates early
    async fn parse_query(
        &self,
        ctx: &ExtensionContext<'_>,
        query: &str,
        variables: &Variables,
        next: NextParseQuery<'_>,
    ) -> ServerResult<ExecutableDocument> {
        let metrics: &Metrics = ctx.data_unchecked();
        let payload_size: &PayloadSize = ctx.data_unchecked();
        let reporter = Reporter::new(ctx);

        let instant = Instant::now();

        // Make sure the request meets a basic size limit before trying to parse it.
        let max_payload_size = reporter.limits.max_query_payload_size as u64
            + reporter.limits.max_tx_payload_size as u64;

        if payload_size.0 > max_payload_size {
            let message = format!("Overall request too large: {} bytes", payload_size.0);
            return Err(reporter.payload_size_error(&message));
        }

        // Document layout of the query
        let doc = next.run(ctx, query, variables).await?;

        // If the query is pure introspection, we don't need to check the limits. Pure introspection
        // queries are queries that only have one operation with one field and that field is a
        // `__schema` query
        if let DocumentOperations::Single(op) = &doc.operations {
            if let [field] = &op.node.selection_set.node.items[..] {
                if let Selection::Field(f) = &field.node {
                    if f.node.name.node == "__schema" {
                        return Ok(doc);
                    }
                }
            }
        }

        let mut traversal =
            LimitsTraversal::new(*payload_size, &reporter, &doc.fragments, variables);

        let res = traversal.check_document(&doc);
        let usage = traversal.finish(query.len() as u32);
        metrics.query_validation_latency(instant.elapsed());
        usage.report(metrics);

        res.map(|()| {
            if ctx.data_opt::<ShowUsage>().is_some() {
                *self.usage.lock().unwrap() = Some(usage);
            }

            doc
        })
    }
}
