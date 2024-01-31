// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

use crate::config::{Limits, ServiceConfig};
use crate::error::{code, graphql_error, graphql_error_at_pos};
use crate::metrics::Metrics;
use async_graphql::extensions::NextParseQuery;
use async_graphql::extensions::NextRequest;
use async_graphql::extensions::{Extension, ExtensionContext, ExtensionFactory};
use async_graphql::parser::types::{
    Directive, ExecutableDocument, Field, FragmentDefinition, Selection, SelectionSet,
};
use async_graphql::{value, Name, Pos, Positioned, Response, ServerResult, Value, Variables};
use async_graphql_value::Value as GqlValue;
use axum::headers;
use axum::http::HeaderName;
use axum::http::HeaderValue;
use once_cell::sync::Lazy;
use std::collections::{BTreeSet, HashMap, VecDeque};
use std::net::SocketAddr;
use std::sync::Arc;
use std::time::Instant;
use sui_graphql_rpc_headers::LIMITS_HEADER;
use tokio::sync::Mutex;
use tracing::info;
use uuid::Uuid;

/// Only display usage information if this header was in the request.
pub(crate) struct ShowUsage;

#[derive(Clone, Debug, Default)]
struct ValidationRes {
    input_nodes: u32,
    output_nodes: u64,
    depth: u32,
    num_variables: u32,
    num_fragments: u32,
    query_payload: u32,
}

#[derive(Debug, Default)]
pub(crate) struct QueryLimitsChecker {
    validation_result: Mutex<Option<ValidationRes>>,
}

pub(crate) const CONNECTION_FIELDS: [&str; 2] = ["edges", "nodes"];

impl headers::Header for ShowUsage {
    fn name() -> &'static HeaderName {
        &LIMITS_HEADER
    }

    fn decode<'i, I>(_: &mut I) -> Result<Self, headers::Error>
    where
        I: Iterator<Item = &'i HeaderValue>,
    {
        Ok(ShowUsage)
    }

    fn encode<E: Extend<HeaderValue>>(&self, _: &mut E) {
        unimplemented!()
    }
}

impl ExtensionFactory for QueryLimitsChecker {
    fn create(&self) -> Arc<dyn Extension> {
        Arc::new(QueryLimitsChecker {
            validation_result: Mutex::new(None),
        })
    }
}

#[derive(Debug)]
struct ComponentCost {
    pub input_nodes: u32,
    pub output_nodes: u64,
    pub depth: u32,
}

impl std::ops::Add for ComponentCost {
    type Output = Self;

    fn add(self, rhs: Self) -> Self::Output {
        Self {
            input_nodes: self.input_nodes + rhs.input_nodes,
            output_nodes: self.output_nodes + rhs.output_nodes,
            depth: self.depth + rhs.depth,
        }
    }
}

#[async_trait::async_trait]
impl Extension for QueryLimitsChecker {
    async fn request(&self, ctx: &ExtensionContext<'_>, next: NextRequest<'_>) -> Response {
        let resp = next.run(ctx).await;
        let validation_result = self.validation_result.lock().await.take();
        if let Some(validation_result) = validation_result {
            resp.extension(
                "usage",
                value! ({
                    "inputNodes": validation_result.input_nodes,
                    "outputNodes": validation_result.output_nodes,
                    "depth": validation_result.depth,
                    "variables": validation_result.num_variables,
                    "fragments": validation_result.num_fragments,
                    "queryPayload": validation_result.query_payload,
                }),
            )
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
        let query_id: &Uuid = ctx.data_unchecked();
        let session_id: &SocketAddr = ctx.data_unchecked();
        let metrics: &Metrics = ctx.data_unchecked();
        let instant = Instant::now();
        let cfg = ctx
            .data::<ServiceConfig>()
            .expect("No service config provided in schema data");
        if query.len() > cfg.limits.max_query_payload_size as usize {
            metrics
                .request_metrics
                .query_payload_too_large_size
                .observe(query.len() as f64);
            info!(
                query_id = %query_id,
                session_id = %session_id,
                error_code = code::BAD_USER_INPUT,
                "Query payload is too large: {}",
                query.len()
            );

            return Err(graphql_error(
                code::BAD_USER_INPUT,
                format!(
                    "Query payload is too large. The maximum allowed is {} bytes",
                    cfg.limits.max_query_payload_size
                ),
            ));
        }

        // Document layout of the query
        let doc = next.run(ctx, query, variables).await?;

        // TODO: Limit the complexity of fragments early on

        let mut running_costs = ComponentCost {
            depth: 0,
            input_nodes: 0,
            output_nodes: 0,
        };
        let mut max_depth_seen = 0;

        // An operation is a query, mutation or subscription consisting of a set of selections
        for (count, (_name, oper)) in doc.operations.iter().enumerate() {
            let sel_set = &oper.node.selection_set;

            // If the query is pure introspection, we don't need to check the limits.
            // Pure introspection queries are queries that only have one operation with one field
            // and that field is a `__schema` query
            if (count == 0) && (sel_set.node.items.len() == 1) {
                if let Some(node) = sel_set.node.items.first() {
                    if let Selection::Field(field) = &node.node {
                        if field.node.name.node == "__schema" {
                            continue;
                        }
                    }
                }
            }

            running_costs.depth = 0;
            self.analyze_selection_set(
                &cfg.limits,
                &doc.fragments,
                sel_set,
                &mut running_costs,
                variables,
                ctx,
            )?;
            max_depth_seen = max_depth_seen.max(running_costs.depth);
        }
        let elapsed = instant.elapsed().as_millis() as u64;

        if ctx.data_opt::<ShowUsage>().is_some() {
            *self.validation_result.lock().await = Some(ValidationRes {
                input_nodes: running_costs.input_nodes,
                output_nodes: running_costs.output_nodes,
                depth: running_costs.depth,
                query_payload: query.len() as u32,
                num_variables: variables.len() as u32,
                num_fragments: doc.fragments.len() as u32,
            });
        }
        metrics.query_validation_latency(elapsed);
        metrics
            .request_metrics
            .input_nodes
            .observe(running_costs.input_nodes as f64);
        metrics
            .request_metrics
            .output_nodes
            .observe(running_costs.output_nodes as f64);
        metrics
            .request_metrics
            .query_depth
            .observe(running_costs.depth as f64);
        metrics
            .request_metrics
            .query_payload_size
            .observe(query.len() as f64);
        Ok(doc)
    }
}

impl QueryLimitsChecker {
    /// Parse the selected fields in one operation and check if it conforms to configured limits.
    fn analyze_selection_set(
        &self,
        limits: &Limits,
        fragment_defs: &HashMap<Name, Positioned<FragmentDefinition>>,
        sel_set: &Positioned<SelectionSet>,
        cost: &mut ComponentCost,
        variables: &Variables,
        ctx: &ExtensionContext<'_>,
    ) -> ServerResult<()> {
        // Use BFS to analyze the query and count the number of nodes and the depth of the query
        struct ToVisit<'s> {
            selection: &'s Positioned<Selection>,
            parent_node_count: u64,
        }

        // Queue to store the nodes at each level
        let mut que = VecDeque::new();

        for selection in sel_set.node.items.iter() {
            que.push_back(ToVisit {
                selection,
                parent_node_count: 1,
            });
            cost.input_nodes += 1;
            check_limits(limits, cost, Some(selection.pos), ctx)?;
        }

        // Track the number of nodes at first level if any
        let mut level_len = que.len();

        while !que.is_empty() {
            // Signifies the start of a new level
            cost.depth += 1;
            check_limits(limits, cost, None, ctx)?;
            while level_len > 0 {
                // Ok to unwrap since we checked for empty queue
                // and level_len > 0
                let ToVisit {
                    selection,
                    parent_node_count,
                } = que.pop_front().unwrap();

                match &selection.node {
                    Selection::Field(f) => {
                        check_directives(&f.node.directives)?;

                        let current_count = estimate_output_nodes_for_curr_node(
                            f,
                            variables,
                            limits.default_page_size,
                        ) * parent_node_count;

                        cost.output_nodes += current_count;

                        for field_sel in f.node.selection_set.node.items.iter() {
                            que.push_back(ToVisit {
                                selection: field_sel,
                                parent_node_count: current_count,
                            });
                            cost.input_nodes += 1;
                            check_limits(limits, cost, Some(field_sel.pos), ctx)?;
                        }
                    }

                    Selection::FragmentSpread(fs) => {
                        let frag_name = &fs.node.fragment_name.node;
                        let frag_def = fragment_defs.get(frag_name).ok_or_else(|| {
                            graphql_error_at_pos(
                                code::INTERNAL_SERVER_ERROR,
                                format!(
                                    "Fragment {} not found but present in fragment list",
                                    frag_name
                                ),
                                fs.pos,
                            )
                        })?;

                        // TODO: this is inefficient as we might loop over same fragment multiple times
                        // Ideally web should cache the costs of fragments we've seen before
                        // Will do as enhancement
                        check_directives(&frag_def.node.directives)?;
                        for selection in frag_def.node.selection_set.node.items.iter() {
                            que.push_back(ToVisit {
                                selection,
                                parent_node_count,
                            });
                            cost.input_nodes += 1;
                            check_limits(limits, cost, Some(selection.pos), ctx)?;
                        }
                    }

                    Selection::InlineFragment(fs) => {
                        check_directives(&fs.node.directives)?;
                        for selection in fs.node.selection_set.node.items.iter() {
                            que.push_back(ToVisit {
                                selection,
                                parent_node_count,
                            });
                            cost.input_nodes += 1;
                            check_limits(limits, cost, Some(selection.pos), ctx)?;
                        }
                    }
                }
                level_len -= 1;
            }
            level_len = que.len();
        }

        Ok(())
    }
}

fn check_limits(
    limits: &Limits,
    cost: &ComponentCost,
    pos: Option<Pos>,
    ctx: &ExtensionContext<'_>,
) -> ServerResult<()> {
    let query_id: &Uuid = ctx.data_unchecked();
    let session_id: &SocketAddr = ctx.data_unchecked();
    let error_code = code::BAD_USER_INPUT;
    if cost.input_nodes > limits.max_query_nodes {
        info!(
            query_id = %query_id,
            session_id = %session_id,
            error_code,
            "Query has too many nodes: {}", cost.input_nodes
        );
        return Err(graphql_error_at_pos(
            error_code,
            format!(
                "Query has too many nodes {}. The maximum allowed is {}",
                cost.input_nodes, limits.max_query_nodes
            ),
            pos.unwrap_or_default(),
        ));
    }

    if cost.depth > limits.max_query_depth {
        info!(
            query_id = %query_id,
            session_id = %session_id,
            error_code,
            "Query has too many levels of nesting: {}", cost.depth
        );
        return Err(graphql_error_at_pos(
            error_code,
            format!(
                "Query has too many levels of nesting {}. The maximum allowed is {}",
                cost.depth, limits.max_query_depth
            ),
            pos.unwrap_or_default(),
        ));
    }

    if cost.output_nodes > limits.max_output_nodes {
        info!(
            query_id = %query_id,
            session_id = %session_id,
            error_code,
            "Query will result in too many output nodes: {}",
            cost.output_nodes
        );
        return Err(graphql_error_at_pos(
            error_code,
                format!(
                "Query will result in too many output nodes. The maximum allowed is {}, estimated {}",
                limits.max_output_nodes, cost.output_nodes
            ),
            pos.unwrap_or_default(),
        ));
    }

    Ok(())
}

// TODO: make this configurable
fn allowed_directives() -> &'static BTreeSet<&'static str> {
    static DIRECTIVES: Lazy<BTreeSet<&str>> =
        Lazy::new(|| BTreeSet::from_iter(["skip", "include"]));

    Lazy::force(&DIRECTIVES)
}

fn check_directives(directives: &[Positioned<Directive>]) -> ServerResult<()> {
    for directive in directives {
        if !allowed_directives().contains(&directive.node.name.node.as_str()) {
            return Err(graphql_error_at_pos(
                code::INTERNAL_SERVER_ERROR,
                format!(
                    "Directive `@{}` is not supported. Supported directives are {}",
                    directive.node.name.node,
                    allowed_directives()
                        .iter()
                        .map(|s| format!("`@{}`", s))
                        .collect::<Vec<_>>()
                        .join(", ")
                ),
                directive.pos,
            ));
        }
    }
    Ok(())
}

/// Given a node, estimate the number of output nodes it will produce.
fn estimate_output_nodes_for_curr_node(
    f: &Positioned<Field>,
    variables: &Variables,
    default_page_size: u64,
) -> u64 {
    if !is_connection(f) {
        1
    } else {
        // If the args 'first' or 'last' is set, then we should use that as the count
        let first_arg = f.node.get_argument("first");
        let last_arg = f.node.get_argument("last");

        extract_limit(first_arg, variables)
            .or_else(|| extract_limit(last_arg, variables))
            .unwrap_or(default_page_size)
    }
}

/// Try to extract a u64 value from the given argument, or return None on failure.
fn extract_limit(value: Option<&Positioned<GqlValue>>, variables: &Variables) -> Option<u64> {
    if let GqlValue::Variable(var) = &value?.node {
        return match variables.get(var) {
            Some(Value::Number(num)) => num.as_u64(),
            _ => None,
        };
    }

    let GqlValue::Number(value) = &value?.node else {
        return None;
    };
    value.as_u64()
}

/// Checks if the given field is a connection field by whether it has 'edges' or 'nodes' selected.
/// This should typically not require checking more than the first element of the selection set
fn is_connection(f: &Positioned<Field>) -> bool {
    for field_sel in f.node.selection_set.node.items.iter() {
        if let Selection::Field(field) = &field_sel.node {
            if CONNECTION_FIELDS.contains(&field.node.name.node.as_str()) {
                return true;
            }
        }
    }
    false
}
