// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

use async_graphql::{Context, InputObject, Object};
use std::sync::Arc;

use crate::{error::RpcError, scope::Scope, task::watermark::Watermarks};

use super::checkpoint::Checkpoint;

/// Identifies a GraphQL query component that is used to determine the range of checkpoints for which data is available (for data that can be tied to a particular checkpoint)
///
/// Both `type_` and `field` are required. The `filter` is optional and provides retention information for filtered queries.
#[derive(InputObject, Debug, Clone, Eq, PartialEq)]
pub(crate) struct RetentionKey {
    /// The GraphQL type to check retention for
    pub(crate) type_: String,

    /// The specific field within the type to check retention for
    pub(crate) field: String,

    /// Optional filter to check retention for filtered queries
    pub(crate) filter: Option<String>,
}

#[derive(Clone)]
pub struct AvailableRange {
    pub scope: Scope,
    pub first: u64,
}

/// Checkpoint range for which data is available.
#[Object]
impl AvailableRange {
    /// Inclusive lower checkpoint for which data is available.
    async fn first(&self) -> Result<Option<Checkpoint>, RpcError> {
        Ok(Checkpoint::with_sequence_number(
            self.scope.clone(),
            Some(self.first),
        ))
    }

    /// Inclusive upper checkpoint for which data is available.
    async fn last(&self) -> Result<Option<Checkpoint>, RpcError> {
        Ok(Checkpoint::with_sequence_number(self.scope.clone(), None))
    }
}

impl AvailableRange {
    /// Get retention information for a specific query type and field
    pub(crate) fn new(
        ctx: &Context<'_>,
        scope: &Scope,
        retention_key: RetentionKey,
    ) -> Result<Self, RpcError> {
        let watermarks: &Arc<Watermarks> = ctx.data()?;
        let pipelines = pipeline(
            &retention_key.type_,
            &retention_key.field,
            retention_key.filter.as_deref(),
        );

        let lo_checkpoint =
            pipelines
                .iter()
                .try_fold(0, |acc: u64, pipeline| -> Result<u64, RpcError> {
                    let watermark = watermarks.pipeline_lo_watermark(pipeline)?;
                    let checkpoint = watermark.checkpoint();
                    Ok(acc.max(checkpoint))
                })?;

        Ok(Self {
            scope: scope.clone(),
            first: lo_checkpoint,
        })
    }
}

/// Maps GraphQL query components to watermark pipeline names.
///
/// Determines which watermark pipelines are relevant for a given GraphQL query.
/// The pipeline names are used to query watermark data to determine the
/// checkpoint sequence range (available range) for which data is available.
///
fn pipeline(type_: &str, field: &str, filter: Option<&str>) -> &'static [&'static str] {
    match (type_, field, filter) {
        // Transaction queries
        ("Query", "transaction", None) => &["tx_digests"],
        ("Query", "transactions", None) => &["tx_digests"],
        ("Query", "transactions", Some("affectedAddress")) => {
            &["tx_digests", "tx_affected_addresses"]
        }
        ("Query", "transactions", Some("affectedObject")) => &["tx_digests", "tx_affected_objects"],
        ("Query", "transactions", Some("function")) => &["tx_digests", "tx_calls"],
        ("Query", "transactions", Some("kind")) => &[
            "tx_digests",
            "tx_kinds",
            "tx_affected_addresses",
            "tx_affected_objects",
        ],
        ("Query", "transactions", Some("sentAddress")) => &[
            "tx_digests",
            "tx_affected_addresses",
            "tx_affected_objects",
            "tx_calls",
        ],
        (_, "transactions", f) => pipeline("Query", "transactions", f),

        // Checkpoint queries
        ("Query", "checkpoints", _) => &["cp_sequence_numbers"],
        ("Query", "checkpoint", None) => pipeline("Query", "checkpoints", None),
        (_, "checkpoints", f) => pipeline("Query", "checkpoints", f),

        // Event queries
        ("Query", "events", None) => &["ev_struct_inst", "ev_emit_mod"],
        ("Query", "events", Some("module")) => &["ev_emit_mod"],
        ("Query", "events", Some("type")) => &["ev_emit_mod"],
        ("Query", "events", Some("sender")) => pipeline("Query", "events", None),
        (_, "events", f) => pipeline("Query", "events", f),

        // Epoch queries
        ("Query", "epochs", _) => &["cp_sequence_numbers"],
        ("Query", "epoch", _) => pipeline("Query", "epochs", None),
        (_, "epochs", f) => pipeline("Query", "epoch", f),

        // Object and package queries
        ("Query", "objectVersions", _) => &["obj_versions"],
        ("Query", "packages", _) => &["cp_sequence_numbers"],

        // Default case
        _ => &[],
    }
}
