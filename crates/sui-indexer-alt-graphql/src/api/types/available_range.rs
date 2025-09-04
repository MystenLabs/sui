// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

use async_graphql::{Context, InputObject, Object};
use std::sync::Arc;

use crate::{
    error::{bad_user_input, RpcError},
    scope::Scope,
    task::watermark::Watermarks,
};

use super::checkpoint::Checkpoint;

#[derive(thiserror::Error, Debug)]
pub(crate) enum Error {
    #[error("Retention range not found for: {0}.{1}({2})")]
    NotFound(String, String, String),
}

/// Identifies a GraphQL query component that is used to determine the range of checkpoints for which data is available
/// (for data that can be tied to a particular checkpoint)
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
    ) -> Result<Self, RpcError<Error>> {
        let watermarks: &Arc<Watermarks> = ctx.data()?;
        let pipeline = pipeline(retention_key)?;

        // TODO: (henry) Make this a Vec<String> instead of a String? If a query uses multiple pipelines, we should use the strictest retention (max lo watermark).
        let lo_watermark = watermarks.pipeline_lo_watermark(&pipeline)?;

        let cp_lo = lo_watermark.checkpoint();

        Ok(Self {
            scope: scope.clone(),
            first: cp_lo,
        })
    }
}

/// Determine the pipeline name based on the query parameters
fn pipeline(retention_key: RetentionKey) -> Result<String, RpcError<Error>> {
    // Map query type, function, and filter to pipeline names
    // TODO: (henry) Could use some suggestions on how to store Query, field, filter to pipeline mapping
    match (
        retention_key.type_.as_str(),
        retention_key.field.as_str(),
        retention_key.filter.as_ref().map(|f| f.as_str()),
    ) {
        ("Query", "transactions", None) => Ok("tx_digests".to_string()),
        ("Query", "transactions", Some("affectedAddresses")) => {
            Ok("tx_affected_addresses".to_string())
        }
        ("Query", "transactions", Some("affectedObjects")) => Ok("tx_affected_objects".to_string()),
        ("Query", "checkpoints", _) => Ok("tx_digests".to_string()),
        ("Query", "events", None) => Ok("ev_struct_inst".to_string()),
        ("Query", "events", Some("module")) => Ok("ev_emit_mod".to_string()),
        ("Query", "events", Some("type")) => Ok("ev_emit_mod".to_string()),
        _ => Err(bad_user_input(Error::NotFound(
            retention_key.type_,
            retention_key.field,
            retention_key.filter.unwrap_or_default(),
        ))),
    }
}
