// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

use crate::config::Limits;
use crate::config::ServiceConfig;
use crate::error::code::BAD_USER_INPUT;
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

static LIMITS_HEADER: HeaderName = HeaderName::from_static("x-sui-rpc-show-usage");

/// Only display usage information if this header was in the request.
pub(crate) struct ShowUsage;

#[derive(Clone, Debug, Default)]
struct ValidationRes {
    num_nodes: u32,
    depth: u32,
    num_variables: u32,
    num_fragments: u32,
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
        // TODO: limit number of variables, fragments, etc
        // TODO: limit/ban directives for now
        // TODO: limit overall query size

        let cfg = ctx
            .data::<ServiceConfig>()
            .expect("No service config provided in schema data");

        if variables.len() > cfg.limits.max_query_variables as usize {
            return Err(ServerError::new(
                format!(
                    "Query has too many variables. The maximum allowed is {}",
                    cfg.limits.max_query_variables
                ),
                None,
            ));
        }

        // Document layout of the query
        let doc = next.run(ctx, query, variables).await?;

        if doc.fragments.len() > cfg.limits.max_query_fragments as usize {
            return Err(ServerError::new(
                format!(
                    "Query has too many fragments definitions. The maximum allowed is {}",
                    cfg.limits.max_query_fragments
                ),
                None,
            ));
        }

        // Only allow one operation
        match doc.operations.iter().count() {
            0 => {
                return Err(graphql_error(BAD_USER_INPUT, "One operation is required"));
            }
            1 => {}
            _ => {
                return Err(graphql_error_at_pos(
                    BAD_USER_INPUT,
                    "Query has too many operations. The maximum allowed is 1",
                    doc.operations.iter().next().unwrap().1.pos,
                ));
            }
        }

        // TODO: Limit the complexity of fragments early on

        // Okay to unwrap since we checked for 1 operation
        let (_name, oper) = doc.operations.iter().next().unwrap();

        let ComponentCost { num_nodes, depth } =
            self.analyze_selection_set(cfg, &doc.fragments, &oper.node.selection_set)?;

        if ctx.data_opt::<ShowUsage>().is_some() {
            *self.validation_result.lock().await = Some(ValidationRes {
                num_nodes,
                depth,
                num_variables: variables.len() as u32,
                num_fragments: doc.fragments.len() as u32,
            });
        }
        if let Some(metrics) = ctx.data_opt::<Arc<RequestMetrics>>() {
            metrics.num_nodes.observe(num_nodes as f64);
            metrics.query_depth.observe(depth as f64);
            metrics.query_payload_size.observe(query.len() as f64);
        }
        Ok(doc)
    }
}

impl QueryLimitsChecker {
    fn analyze_selection_set(
        &self,
        cfg: &ServiceConfig,
        fragment_defs: &HashMap<Name, Positioned<FragmentDefinition>>,
        sel_set: &Positioned<SelectionSet>,
    ) -> ServerResult<ComponentCost> {
        // Use BFS to analyze the query and
        // count the number of nodes and the depth of the query

        // Queue to store the nodes at each level
        let mut que = VecDeque::new();
        // Number of nodes in the query
        let mut num_nodes: u32 = 0;
        // Depth of the query
        let mut depth: u32 = 0;

        for top_level_sel in sel_set.node.items.iter() {
            que.push_back(top_level_sel);
            num_nodes += 1;
            check_limits(&cfg.limits, num_nodes, depth, Some(top_level_sel.pos))?;
        }

        // Track the number of nodes at first level if any
        let mut level_len = que.len();

        while !que.is_empty() {
            // Signifies the start of a new level
            depth += 1;
            check_limits(&cfg.limits, num_nodes, depth, None)?;
            while level_len > 0 {
                // Ok to unwrap since we checked for empty queue
                // and level_len > 0
                let curr_sel = que.pop_front().unwrap();

                match &curr_sel.node {
                    Selection::Field(f) => {
                        for field_sel in f.node.selection_set.node.items.iter() {
                            que.push_back(field_sel);
                            num_nodes += 1;
                            check_limits(&cfg.limits, num_nodes, depth, Some(field_sel.pos))?;
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
                        for frag_sel in frag_def.node.selection_set.node.items.iter() {
                            que.push_back(frag_sel);
                            num_nodes += 1;
                            check_limits(&cfg.limits, num_nodes, depth, Some(frag_sel.pos))?;
                        }
                    }
                    Selection::InlineFragment(fs) => {
                        for in_frag_sel in fs.node.selection_set.node.items.iter() {
                            que.push_back(in_frag_sel);
                            num_nodes += 1;
                            check_limits(&cfg.limits, num_nodes, depth, Some(in_frag_sel.pos))?;
                        }
                    }
                }
                level_len -= 1;
            }
            level_len = que.len();
        }
        Ok(ComponentCost { num_nodes, depth })
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
