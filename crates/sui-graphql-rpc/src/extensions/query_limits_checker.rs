// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

use crate::config::Limits;
use crate::config::ServiceConfig;
use crate::error::code;
use crate::error::code::INTERNAL_SERVER_ERROR;
use crate::error::graphql_error;
use crate::error::graphql_error_at_pos;
use crate::metrics::RequestMetrics;
use async_graphql::extensions::NextParseQuery;
use async_graphql::extensions::NextRequest;
use async_graphql::parser::types::ExecutableDocument;
use async_graphql::parser::types::FragmentDefinition;
use async_graphql::parser::types::Selection;
use async_graphql::parser::types::SelectionSet;
use async_graphql::value;
use async_graphql::Name;
use async_graphql::Pos;
use async_graphql::Positioned;
use async_graphql::Response;
use async_graphql::ServerResult;
use async_graphql::Variables;
use async_graphql::{
    extensions::{Extension, ExtensionContext, ExtensionFactory},
    ServerError,
};
use axum::headers;
use axum::http::HeaderName;
use axum::http::HeaderValue;
use std::collections::HashMap;
use std::collections::VecDeque;
use std::sync::Arc;
use tokio::sync::Mutex;

pub static LIMITS_HEADER: HeaderName = HeaderName::from_static("x-sui-rpc-show-usage");

/// Only display usage information if this header was in the request.
pub(crate) struct ShowUsage;

#[derive(Clone, Debug, Default)]
struct ValidationRes {
    num_nodes: u32,
    depth: u32,
    num_variables: u32,
    num_fragments: u32,
    query_payload: u32,
}

#[derive(Debug, Default)]
pub(crate) struct QueryLimitsChecker {
    validation_result: Mutex<Option<ValidationRes>>,
}

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

struct ComponentCost {
    pub num_nodes: u32,
    pub depth: u32,
}

impl std::ops::Add for ComponentCost {
    type Output = Self;

    fn add(self, rhs: Self) -> Self::Output {
        Self {
            num_nodes: self.num_nodes + rhs.num_nodes,
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
                    "nodes": validation_result.num_nodes,
                    "depth": validation_result.depth,
                    "variables": validation_result.num_variables,
                    "fragments": validation_result.num_fragments,
                    "query_payload": validation_result.query_payload,
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
        let cfg = ctx
            .data::<ServiceConfig>()
            .expect("No service config provided in schema data");

        if query.len() > cfg.limits.max_query_payload_size as usize {
            return Err(graphql_error(
                code::GRAPHQL_VALIDATION_FAILED,
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
            num_nodes: 0,
        };
        let mut max_depth_seen = 0;

        for (_name, oper) in doc.operations.iter() {
            running_costs.depth = 0;
            self.analyze_selection_set(
                &cfg.limits,
                &doc.fragments,
                &oper.node.selection_set,
                &mut running_costs,
            )?;
            max_depth_seen = max_depth_seen.max(running_costs.depth);
        }

        if ctx.data_opt::<ShowUsage>().is_some() {
            *self.validation_result.lock().await = Some(ValidationRes {
                num_nodes: running_costs.num_nodes,
                depth: running_costs.depth,
                query_payload: query.len() as u32,
                num_variables: variables.len() as u32,
                num_fragments: doc.fragments.len() as u32,
            });
        }
        if let Some(metrics) = ctx.data_opt::<Arc<RequestMetrics>>() {
            metrics.num_nodes.observe(running_costs.num_nodes as f64);
            metrics.query_depth.observe(running_costs.depth as f64);
            metrics.query_payload_size.observe(query.len() as f64);
        }
        Ok(doc)
    }
}

impl QueryLimitsChecker {
    fn analyze_selection_set(
        &self,
        limits: &Limits,
        fragment_defs: &HashMap<Name, Positioned<FragmentDefinition>>,
        sel_set: &Positioned<SelectionSet>,
        cost: &mut ComponentCost,
    ) -> ServerResult<()> {
        // Use BFS to analyze the query and count the number of nodes and the depth of the query

        // Queue to store the nodes at each level
        let mut que = VecDeque::new();

        for top_level_sel in sel_set.node.items.iter() {
            que.push_back(top_level_sel);
            cost.num_nodes += 1;
            check_limits(limits, cost.num_nodes, cost.depth, Some(top_level_sel.pos))?;
        }

        // Track the number of nodes at first level if any
        let mut level_len = que.len();

        while !que.is_empty() {
            // Signifies the start of a new level
            cost.depth += 1;
            check_limits(limits, cost.num_nodes, cost.depth, None)?;
            while level_len > 0 {
                // Ok to unwrap since we checked for empty queue
                // and level_len > 0
                let curr_sel = que.pop_front().unwrap();

                match &curr_sel.node {
                    Selection::Field(f) => {
                        if !f.node.directives.is_empty() {
                            return Err(graphql_error_at_pos(
                                INTERNAL_SERVER_ERROR,
                                "Fields with directives are not supported",
                                f.pos,
                            ));
                        }
                        for field_sel in f.node.selection_set.node.items.iter() {
                            que.push_back(field_sel);
                            cost.num_nodes += 1;
                            check_limits(limits, cost.num_nodes, cost.depth, Some(field_sel.pos))?;
                        }
                    }
                    Selection::FragmentSpread(fs) => {
                        let frag_name = &fs.node.fragment_name.node;
                        let frag_def = fragment_defs.get(frag_name).ok_or_else(|| {
                            graphql_error_at_pos(
                                INTERNAL_SERVER_ERROR,
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
                        if !frag_def.node.directives.is_empty() {
                            return Err(graphql_error_at_pos(
                                INTERNAL_SERVER_ERROR,
                                "Fragments with directives are not supported",
                                frag_def.pos,
                            ));
                        }
                        for frag_sel in frag_def.node.selection_set.node.items.iter() {
                            que.push_back(frag_sel);
                            cost.num_nodes += 1;
                            check_limits(limits, cost.num_nodes, cost.depth, Some(frag_sel.pos))?;
                        }
                    }
                    Selection::InlineFragment(fs) => {
                        if !fs.node.directives.is_empty() {
                            return Err(graphql_error_at_pos(
                                INTERNAL_SERVER_ERROR,
                                "Inline fragments with directives are not supported",
                                fs.pos,
                            ));
                        }
                        for in_frag_sel in fs.node.selection_set.node.items.iter() {
                            que.push_back(in_frag_sel);
                            cost.num_nodes += 1;
                            check_limits(
                                limits,
                                cost.num_nodes,
                                cost.depth,
                                Some(in_frag_sel.pos),
                            )?;
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

fn check_limits(limits: &Limits, nodes: u32, depth: u32, pos: Option<Pos>) -> ServerResult<()> {
    if nodes > limits.max_query_nodes {
        return Err(ServerError::new(
            format!(
                "Query has too many nodes. The maximum allowed is {}",
                limits.max_query_nodes
            ),
            pos,
        ));
    }

    if depth > limits.max_query_depth {
        return Err(ServerError::new(
            format!(
                "Query has too many levels of nesting. The maximum allowed is {}",
                limits.max_query_depth
            ),
            pos,
        ));
    }

    Ok(())
}
